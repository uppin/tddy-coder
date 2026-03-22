//! Tests for --conversation-output: raw agent bytes written to file.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::fs;
use std::sync::Arc;
use tddy_core::{CodingBackend, CursorBackend, MockBackend, SharedBackend, WorkflowEngine};

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

    let _ = run_plan_with_conversation_output(&engine, "Build auth", &tmp, None, None).await;

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
