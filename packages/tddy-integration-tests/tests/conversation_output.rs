//! Tests for --conversation-output: raw agent bytes written to file.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::fs;
use std::sync::{Arc, Mutex};
use tddy_core::{
    AgentOutputSink, ClaudeCodeBackend, CodingBackend, CursorBackend, MockBackend, SessionMode,
    SharedBackend, WorkflowEngine,
};

use common::run_plan_with_conversation_output;

/// Plan output as JSON (tddy-tools submit format). MockBackend stores this via store_submit_result.
const PLAN_OUTPUT: &str = r##"{"goal":"plan","prd":"# Feature PRD\n\n## Summary\nTest feature.\n\n## TODO\n\n- [ ] Task 1"}"##;

const RAW_STREAM: &str = r#"{"type":"system","session_id":"sess-1"}
{"type":"result","result":"output","session_id":"sess-1"}"#;

/// When conversation_output_path is set, MockBackend writes raw bytes to the file.
#[tokio::test]
async fn mock_backend_writes_conversation_to_file_when_path_set() {
    let tmp = std::env::temp_dir().join("tddy-conv-output-mock");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let output_file = tmp.join("conversation.ndjson");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_raw_stream(PLAN_OUTPUT, RAW_STREAM);

    let storage_dir = std::env::temp_dir().join("tddy-conv-output-mock-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let result = run_plan_with_conversation_output(
        &engine,
        "Build auth",
        &tmp,
        None,
        Some(output_file.clone()),
    )
    .await;

    assert!(result.is_ok(), "plan should succeed: {:?}", result);

    let content = fs::read_to_string(&output_file).expect("conversation file should exist");
    assert_eq!(content, RAW_STREAM, "file should contain raw stream bytes");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// When conversation_output_path is None, no file is created.
#[tokio::test]
async fn mock_backend_creates_no_file_when_path_not_set() {
    // Given
    let tmp = std::env::temp_dir().join("tddy-conv-output-none");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let output_file = tmp.join("conversation.ndjson");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-conv-output-none-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    // When
    let _ = run_plan_with_conversation_output(&engine, "Build auth", &tmp, None, None).await;

    // Then
    assert!(
        !output_file.exists(),
        "no conversation file should be created when path not set"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// CursorBackend writes raw stdout bytes to file when conversation_output_path is set.
#[tokio::test]
#[cfg(unix)]
async fn cursor_backend_writes_raw_stream_to_conversation_output_file() {
    let tmp = std::env::temp_dir().join("tddy-conv-output-cursor");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let output_file = tmp.join("cursor-conv.ndjson");

    let expected_raw = r#"{"type":"system","thread_id":"t1"}
{"type":"result","result":"done","session_id":"t1"}"#;

    let script = r##"#!/bin/sh
printf '%s\n' '{"type":"system","thread_id":"t1"}'
printf '%s\n' '{"type":"result","result":"done","session_id":"t1"}'
exit 0
"##
    .to_string();
    let script_path = tmp.join("agent");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CursorBackend::with_path(script_path);
    let mut req = common::stub_invoke_request("test", "plan");
    req.conversation_output_path = Some(output_file.clone());

    let result = backend.invoke(req).await;
    assert!(result.is_ok(), "invoke should succeed: {:?}", result);

    let content = fs::read_to_string(&output_file).expect("conversation file should exist");
    let lines: Vec<&str> = content.trim().lines().collect();
    assert!(
        lines.len() >= 3,
        "file should have request + stream lines, got {} lines",
        lines.len()
    );
    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse request entry");
    assert_eq!(
        first["type"], "tddy-request",
        "first line should be the request"
    );
    assert_eq!(first["prompt"], "test");
    assert_eq!(first["goal"], "Plan");

    let stream_lines = lines[1..].join("\n");
    assert_eq!(
        stream_lines.trim(),
        expected_raw.trim(),
        "remaining lines should contain raw NDJSON stream"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// Resumed invocations of a long-lived session must not lose live agent output
// ---------------------------------------------------------------------------
//
// A resumed backend invocation skips echoing lines up to `skip_until_line`, a count read
// from the already-persisted `conversation_output_path` file (see cursor.rs / claude.rs
// `invoke()`). That count accumulates across every prior turn of a session, while a fresh
// invocation's own stream always starts counting from line 1 — so once a session has had a
// few turns, the persisted total permanently exceeds anything a single new invocation could
// ever produce, and live agent output is silently dropped for the rest of the session's life.
// Reproduced live: a pr-stack session's `analyze-stack` retry started with 282 already-persisted
// lines but only emitted 56 of its own, so its entire output was skipped and never reached the
// presenter's `AgentOutput` stream (and therefore never reached the web chat UI).

fn write_executable_script(path: &std::path::Path, body: &str) {
    fs::write(path, body).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// A sink that records every chunk it's given, for asserting live-streamed agent output.
fn recording_agent_output_sink() -> (AgentOutputSink, Arc<Mutex<Vec<String>>>) {
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let recorded = chunks.clone();
    let sink = AgentOutputSink::new(move |s: &str| {
        recorded.lock().unwrap().push(s.to_string());
    });
    (sink, chunks)
}

/// A `conversation_output_path` file with `line_count` already-persisted, non-empty lines —
/// simulating a long-lived session with many prior turns already logged.
fn conversation_file_with_prior_history(path: &std::path::Path, line_count: usize) {
    let mut content = String::new();
    for i in 0..line_count {
        content.push_str(&format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"prior turn line {i}"}}]}}}}"#
        ));
        content.push('\n');
    }
    fs::write(path, content).expect("write prior conversation history");
}

#[tokio::test]
#[cfg(unix)]
async fn cursor_backend_streams_agent_output_on_resume_of_a_session_with_a_long_conversation_history(
) {
    // Given — a session that already has 282 persisted lines from prior turns (matches the
    // live pr-stack reproduction), and this invocation's own fresh output is a short 2-line
    // NDJSON stream (system init + one assistant text chunk)
    let tmp = std::env::temp_dir().join("tddy-cursor-resume-long-history");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let conversation_output_path = tmp.join("conversation.jsonl");
    conversation_file_with_prior_history(&conversation_output_path, 282);

    let script = r#"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"thread-1"}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"Analysis complete: two PRs needed."}]}}'
exit 0
"#;
    let script_path = tmp.join("agent");
    write_executable_script(&script_path, script);

    let (sink, recorded) = recording_agent_output_sink();
    let backend = CursorBackend::with_path(script_path);
    let mut req = common::stub_invoke_request("Analyze the stack", "plan");
    req.session = Some(SessionMode::Resume("thread-1".to_string()));
    req.agent_output = true;
    req.agent_output_sink = Some(sink);
    req.conversation_output_path = Some(conversation_output_path);

    // When
    let result = backend.invoke(req).await;

    // Then
    assert!(result.is_ok(), "invoke should succeed: {:?}", result);
    let chunks = recorded.lock().unwrap();
    assert!(
        chunks.iter().any(|c| c.contains("Analysis complete")),
        "expected this invocation's own new output to be live-streamed despite 282 lines of \
         prior history, but no matching chunk was recorded: {:?}",
        *chunks
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
#[cfg(unix)]
async fn claude_backend_streams_agent_output_on_resume_of_a_session_with_a_long_conversation_history(
) {
    // Given — same scenario as the Cursor case above, for Claude's identical skip_until_line logic
    let tmp = std::env::temp_dir().join("tddy-claude-resume-long-history");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let conversation_output_path = tmp.join("conversation.jsonl");
    conversation_file_with_prior_history(&conversation_output_path, 282);

    let script = r#"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"sess-1"}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"Analysis complete: two PRs needed."}]}}'
printf '%s\n' '{"type":"result","subtype":"success","result":"Analysis complete: two PRs needed.","session_id":"sess-1","is_error":false}'
exit 0
"#;
    let script_path = tmp.join("claude");
    write_executable_script(&script_path, script);

    let (sink, recorded) = recording_agent_output_sink();
    let backend = ClaudeCodeBackend::with_path(script_path);
    let mut req = common::stub_invoke_request("Analyze the stack", "plan");
    req.session = Some(SessionMode::Resume("sess-1".to_string()));
    req.agent_output = true;
    req.agent_output_sink = Some(sink);
    req.conversation_output_path = Some(conversation_output_path);

    // When
    let result = backend.invoke(req).await;

    // Then
    assert!(result.is_ok(), "invoke should succeed: {:?}", result);
    let chunks = recorded.lock().unwrap();
    assert!(
        chunks.iter().any(|c| c.contains("Analysis complete")),
        "expected this invocation's own new output to be live-streamed despite 282 lines of \
         prior history, but no matching chunk was recorded: {:?}",
        *chunks
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
