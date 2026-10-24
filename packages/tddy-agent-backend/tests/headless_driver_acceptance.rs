//! `HeadlessDriver` — a coding agent's turns over its headless CLI, shaped like an ACP turn.
//!
//! Each CLI is a fake: a shell script that appends its argv to a record file and prints the event
//! stream the real CLI prints (claude / cursor stream-json, codex JSONL). Real CLIs are not run.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_agent_backend::{
    AcpStopReason, AcpUpdate, AcpUpdateSink, ConversationDriver, HeadlessCommand, HeadlessDriver,
    HeadlessHarness,
};

const TURN_DEADLINE: Duration = Duration::from_secs(10);

/// A fake CLI in its own directory: prints `stream` on every invocation (after `sleep_secs`), and
/// records each invocation's arguments, one line per call.
struct FakeCli {
    dir: tempfile::TempDir,
}

fn a_fake_cli(stream: &[serde_json::Value], sleep_secs: u32) -> FakeCli {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("stream.jsonl");
    std::fs::write(
        &output,
        stream
            .iter()
            .map(|event| event.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .expect("write stream");
    let script = dir.path().join("cli");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{record}'\nsleep {sleep_secs}\ncat '{output}'\n",
            record = dir.path().join("argv.log").display(),
            output = output.display(),
        ),
    )
    .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    FakeCli { dir }
}

impl FakeCli {
    fn command(&self) -> HeadlessCommand {
        HeadlessCommand {
            program: self.dir.path().join("cli"),
            cwd: Some(self.dir.path().to_path_buf()),
            ..HeadlessCommand::default()
        }
    }

    /// The arguments of each invocation, in order.
    fn invocations(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn claude_stream(session: &str, text: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "type": "system", "subtype": "init", "session_id": session }),
        serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] }, "session_id": session }),
        serde_json::json!({ "type": "result", "subtype": "success", "result": text, "session_id": session }),
    ]
}

fn codex_stream(thread: &str, text: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "type": "thread.started", "thread_id": thread }),
        serde_json::json!({ "type": "item.completed", "item": { "type": "agent_message", "text": text } }),
    ]
}

async fn a_turn(driver: &HeadlessDriver, prompt: &str) -> tddy_agent_backend::AcpTurnEnd {
    tokio::time::timeout(TURN_DEADLINE, driver.prompt_turn(prompt.to_string(), None))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds")
}

fn collecting() -> (AcpUpdateSink, Arc<Mutex<Vec<AcpUpdate>>>) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let sink_updates = Arc::clone(&updates);
    let sink: AcpUpdateSink = Arc::new(move |update| {
        sink_updates.lock().expect("updates").push(update);
    });
    (sink, updates)
}

#[tokio::test]
async fn a_claude_turn_returns_its_final_text_and_the_next_turn_resumes_its_session() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "Fixed it."), 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Claude, cli.command());

    // When
    let first = a_turn(&driver, "Fix the bug").await;
    a_turn(&driver, "Now add a test").await;

    // Then
    assert_eq!(first.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(first.text, "Fixed it.");
    assert_eq!(driver.session_id().as_deref(), Some("cl-1"));
    let calls = cli.invocations();
    assert!(
        calls[0].contains("-p") && calls[0].contains("stream-json"),
        "{}",
        calls[0]
    );
    assert!(calls[1].contains("--resume cl-1"), "{}", calls[1]);
}

#[tokio::test]
async fn a_cursor_turn_returns_its_final_text_and_the_next_turn_resumes_its_chat() {
    // Given
    let cli = a_fake_cli(&claude_stream("cu-1", "Renamed."), 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Cursor, cli.command());

    // When
    let first = a_turn(&driver, "Rename it").await;
    a_turn(&driver, "And the tests").await;

    // Then
    assert_eq!(first.text, "Renamed.");
    let calls = cli.invocations();
    assert!(calls[1].contains("--resume cu-1"), "{}", calls[1]);
}

#[tokio::test]
async fn a_codex_turn_returns_its_final_text_and_the_next_turn_resumes_its_thread() {
    // Given
    let cli = a_fake_cli(&codex_stream("th-1", "Done."), 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Codex, cli.command());

    // When
    let first = a_turn(&driver, "Do it").await;
    a_turn(&driver, "Again").await;

    // Then
    assert_eq!(first.text, "Done.");
    let calls = cli.invocations();
    assert!(
        calls[0].starts_with("exec") && calls[0].contains("--json"),
        "{}",
        calls[0]
    );
    assert!(calls[1].contains("resume th-1"), "{}", calls[1]);
}

#[tokio::test]
async fn cancelling_a_headless_turn_kills_the_cli_and_returns_cancelled() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "never"), 30);
    let driver = Arc::new(HeadlessDriver::new(HeadlessHarness::Claude, cli.command()));
    let turn_driver = Arc::clone(&driver);
    let turn = tokio::spawn(async move { turn_driver.prompt_turn("Work".to_string(), None).await });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // When
    driver.cancel_turn().await.expect("cancel");

    // Then
    let ended = tokio::time::timeout(Duration::from_secs(5), turn)
        .await
        .expect("the turn ended soon after cancel")
        .expect("turn task")
        .expect("a cancelled turn is not an error");
    assert_eq!(ended.stop_reason, AcpStopReason::Cancelled);
}

#[tokio::test]
async fn headless_tool_calls_become_transcript_updates() {
    // Given
    let cli = a_fake_cli(
        &[
            serde_json::json!({ "type": "system", "subtype": "init", "session_id": "cl-1" }),
            serde_json::json!({ "type": "assistant", "message": { "content": [
                { "type": "tool_use", "id": "tu-1", "name": "Read", "input": { "file_path": "src/lib.rs" } }
            ] }, "session_id": "cl-1" }),
            serde_json::json!({ "type": "result", "subtype": "success", "result": "Read it.", "session_id": "cl-1" }),
        ],
        0,
    );
    let driver = HeadlessDriver::new(HeadlessHarness::Claude, cli.command());
    let (sink, updates) = collecting();

    // When
    tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt_turn("Read lib.rs".to_string(), Some(sink)),
    )
    .await
    .expect("in time")
    .expect("the turn succeeds");

    // Then
    let updates = updates.lock().expect("updates");
    assert!(
        updates.iter().any(|update| matches!(update,
            AcpUpdate::ToolCall { id, title, .. } if id == "tu-1" && title.starts_with("Read"))),
        "{updates:?}"
    );
}

#[tokio::test]
async fn a_codex_turns_tool_calls_arrive_at_turn_end() {
    // Given
    let mut stream = codex_stream("th-1", "Tests pass.");
    stream.insert(
        1,
        serde_json::json!({ "type": "item.completed", "item": {
            "id": "item-1", "type": "command_execution", "command": "cargo test", "exit_code": 0
        } }),
    );
    let cli = a_fake_cli(&stream, 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Codex, cli.command());
    let (sink, updates) = collecting();

    // When
    tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt_turn("Run tests".to_string(), Some(sink)),
    )
    .await
    .expect("in time")
    .expect("the turn succeeds");

    // Then
    let updates = updates.lock().expect("updates");
    assert!(
        updates.iter().any(|update| matches!(update,
            AcpUpdate::ToolCall { title, .. } if title.contains("cargo test"))),
        "{updates:?}"
    );
}

#[tokio::test]
async fn the_model_and_the_permission_prompt_tool_reach_claudes_argv() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "ok"), 0);
    let driver = HeadlessDriver::new(
        HeadlessHarness::Claude,
        HeadlessCommand {
            model: Some("sonnet".to_string()),
            permission_prompt_tool: Some("mcp__tddy-tools__approval_prompt".to_string()),
            disallowed_tools: vec!["Bash".to_string(), "Edit".to_string()],
            ..cli.command()
        },
    );

    // When
    a_turn(&driver, "Go").await;

    // Then
    let call = &cli.invocations()[0];
    assert!(call.contains("--model sonnet"), "{call}");
    assert!(
        call.contains("--permission-prompt-tool mcp__tddy-tools__approval_prompt"),
        "{call}"
    );
    assert!(call.contains("--disallowedTools"), "{call}");
    let _ = cli.path();
}

#[test]
fn harness_ids_round_trip() {
    for harness in [
        HeadlessHarness::Claude,
        HeadlessHarness::Cursor,
        HeadlessHarness::Codex,
    ] {
        assert_eq!(HeadlessHarness::from_id(harness.id()), Some(harness));
    }
    assert_eq!(HeadlessHarness::from_id("claude-acp"), None);
}

// ---------------------------------------------------------------------------
// Warp's headless runner, `oz agent run` — replaying streams captured from a real `oz`
// (v0.2026.10.21, 2026-10-24) under tests/fixtures/oz/
// ---------------------------------------------------------------------------

fn an_oz_capture(name: &str) -> Vec<serde_json::Value> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/oz")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).expect("a captured ndjson line"))
        .collect()
}

#[tokio::test]
async fn an_oz_turn_returns_its_text_and_the_next_turn_continues_its_conversation() {
    // Given
    let cli = a_fake_cli(&an_oz_capture("text_turn.ndjson"), 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Oz, cli.command());

    // When
    let first = a_turn(&driver, "Reply with ok").await;
    a_turn(&driver, "And again").await;

    // Then
    assert_eq!(first.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(first.text, "ok\n");
    assert_eq!(driver.session_id().as_deref(), Some("conv-0001"));
    let calls = cli.invocations();
    assert!(
        calls[0].starts_with("agent run") && calls[0].contains("--output-format ndjson"),
        "{}",
        calls[0]
    );
    assert!(
        calls[0].contains("-p Reply with ok") || calls[0].contains("--prompt Reply with ok"),
        "{}",
        calls[0]
    );
    assert!(
        !calls[0].contains("--conversation"),
        "a first turn opens a conversation: {}",
        calls[0]
    );
    assert!(
        calls[1].contains("--conversation conv-0001"),
        "{}",
        calls[1]
    );
}

#[tokio::test]
async fn an_oz_turns_commands_and_edits_become_tool_calls_with_their_results() {
    // Given
    let cli = a_fake_cli(&an_oz_capture("tool_turn.ndjson"), 0);
    let driver = HeadlessDriver::new(HeadlessHarness::Oz, cli.command());
    let (sink, updates) = collecting();

    // When
    let turn = tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt_turn("Do it".to_string(), Some(sink)),
    )
    .await
    .expect("in time")
    .expect("the turn succeeds");

    // Then
    assert!(turn.text.starts_with("Ran `ls -la`"), "{}", turn.text);
    let updates = updates.lock().expect("updates");
    let titles: Vec<&str> = updates
        .iter()
        .filter_map(|update| match update {
            AcpUpdate::ToolCall { title, .. } => Some(title.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["ls -la", "Create the requested text file."]);
    let completed = updates
        .iter()
        .filter(|update| {
            matches!(
                update,
                AcpUpdate::ToolCallUpdate {
                    status: Some(tddy_agent_backend::AcpToolStatus::Completed),
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        completed, 2,
        "each tool_result completes its call: {updates:?}"
    );
}

#[tokio::test]
async fn the_worktree_model_mcp_and_profile_reach_ozs_argv() {
    // Given
    let cli = a_fake_cli(&an_oz_capture("text_turn.ndjson"), 0);
    let mcp = cli.path().join("mcp.json");
    std::fs::write(&mcp, "{}").expect("mcp config");
    let driver = HeadlessDriver::new(
        HeadlessHarness::Oz,
        HeadlessCommand {
            model: Some("claude-sonnet".to_string()),
            mcp_config: Some(mcp.clone()),
            profile: Some("tddy-subagent".to_string()),
            ..cli.command()
        },
    );

    // When
    a_turn(&driver, "Go").await;

    // Then
    let call = &cli.invocations()[0];
    assert!(
        call.contains(&format!("-C {}", cli.path().display())),
        "{call}"
    );
    assert!(call.contains("--model claude-sonnet"), "{call}");
    assert!(call.contains(&format!("--mcp {}", mcp.display())), "{call}");
    assert!(call.contains("--profile tddy-subagent"), "{call}");
}
