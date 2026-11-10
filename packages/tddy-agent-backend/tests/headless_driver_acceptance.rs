//! `HeadlessDriver` — a coding agent's turns over its headless CLI, shaped like an ACP turn.
//!
//! Each CLI is a fake: a shell script that appends its argv to a record file and prints the event
//! stream the real CLI prints (claude / cursor stream-json, codex JSONL). Real CLIs are not run.
//! What a turn is spawned with comes from a fixture renderer standing in for the agent's spec (the
//! specs crate, which implements the real one, cannot be named here; the argv each real spec renders
//! is pinned by its render goldens), so what these tests pin is what the driver hands the renderer
//! and what it does with the stream the adapter reads.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_agent_backend::adapters::{
    AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer, HeadlessTurn,
    CLAUDE_STREAM_JSON, CODEX_JSONL, CURSOR_STREAM, OZ_NDJSON,
};
use tddy_agent_backend::{
    AcpStopReason, AcpUpdate, AcpUpdateSink, ConversationDriver, HeadlessCommand, HeadlessDriver,
    McpServerSpec,
};
use tddy_session_store::error::BackendError;

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

/// A renderer standing in for an agent's spec: `-p <prompt>`, `--resume <session>` on a resumed turn,
/// `--model <model>`, run through the adapter it names, recording every turn it renders.
#[derive(Debug)]
struct AFixtureRenderer {
    agent: &'static str,
    adapter: &'static str,
    program: PathBuf,
    turns: Mutex<Vec<HeadlessTurn>>,
}

impl HeadlessRenderer for AFixtureRenderer {
    fn agent_id(&self) -> &str {
        self.agent
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        None
    }

    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError> {
        self.turns.lock().expect("turns").push(turn.clone());
        let mut argv = vec!["-p".to_string(), turn.prompt.clone()];
        if let Some(model) = &turn.model {
            argv.extend(["--model".to_string(), model.clone()]);
        }
        if let Some(session) = &turn.session {
            if session.is_resume() {
                argv.extend(["--resume".to_string(), session.session_id().to_string()]);
            }
        }
        Ok(HeadlessInvocation {
            program: self.program.clone(),
            argv,
            env: Vec::new(),
            files: Vec::new(),
            adapter: AdapterRegistry::builtin()
                .get(self.adapter)
                .expect("the adapter is registered"),
            exit_ok_if_structured: false,
        })
    }
}

impl FakeCli {
    fn command(&self) -> HeadlessCommand {
        HeadlessCommand {
            cwd: Some(self.dir.path().to_path_buf()),
            scratch_dir: Some(self.dir.path().join("conversation")),
            ..HeadlessCommand::default()
        }
    }

    /// A conversation of `agent`, whose stream the adapter `adapter` reads.
    fn driver(
        &self,
        agent: &'static str,
        adapter: &'static str,
        command: HeadlessCommand,
    ) -> (HeadlessDriver, Arc<AFixtureRenderer>) {
        let renderer = Arc::new(AFixtureRenderer {
            agent,
            adapter,
            program: self.dir.path().join("cli"),
            turns: Mutex::new(Vec::new()),
        });
        (
            HeadlessDriver::new(HeadlessCommand {
                renderer: Some(renderer.clone()),
                ..command
            }),
            renderer,
        )
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
    let (driver, _) = cli.driver("claude", CLAUDE_STREAM_JSON, cli.command());

    // When
    let first = a_turn(&driver, "Fix the bug").await;
    a_turn(&driver, "Now add a test").await;

    // Then
    assert_eq!(first.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(first.text, "Fixed it.");
    assert_eq!(driver.session_id().as_deref(), Some("cl-1"));
    let calls = cli.invocations();
    assert_eq!(calls[0], "-p Fix the bug");
    assert_eq!(calls[1], "-p Now add a test --resume cl-1");
}

#[tokio::test]
async fn a_cursor_turn_returns_its_final_text_and_the_next_turn_resumes_its_chat() {
    // Given
    let cli = a_fake_cli(&claude_stream("cu-1", "Renamed."), 0);
    let (driver, _) = cli.driver("cursor", CURSOR_STREAM, cli.command());

    // When
    let first = a_turn(&driver, "Rename it").await;
    a_turn(&driver, "And the tests").await;

    // Then
    assert_eq!(first.text, "Renamed.");
    let calls = cli.invocations();
    assert_eq!(calls[1], "-p And the tests --resume cu-1");
}

#[tokio::test]
async fn a_codex_turn_returns_its_final_text_and_the_next_turn_resumes_its_thread() {
    // Given
    let cli = a_fake_cli(&codex_stream("th-1", "Done."), 0);
    let (driver, _) = cli.driver("codex", CODEX_JSONL, cli.command());

    // When
    let first = a_turn(&driver, "Do it").await;
    a_turn(&driver, "Again").await;

    // Then
    assert_eq!(first.text, "Done.");
    let calls = cli.invocations();
    assert_eq!(calls[0], "-p Do it");
    assert_eq!(calls[1], "-p Again --resume th-1");
}

#[tokio::test]
async fn cancelling_a_headless_turn_kills_the_cli_and_returns_cancelled() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "never"), 30);
    let (driver, _) = cli.driver("claude", CLAUDE_STREAM_JSON, cli.command());
    let driver = Arc::new(driver);
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
async fn a_conversation_turn_that_outlives_its_deadline_fails_and_its_cli_is_killed() {
    // Given a CLI that would answer only after 30s, in a conversation whose turns may run 1s
    let cli = a_fake_cli(&claude_stream("cl-1", "never"), 30);
    let (driver, _) = cli.driver(
        "claude",
        CLAUDE_STREAM_JSON,
        HeadlessCommand {
            turn_deadline: Some(Duration::from_secs(1)),
            ..cli.command()
        },
    );

    // When a turn runs
    let ended = tokio::time::timeout(TURN_DEADLINE, driver.prompt_turn("Work".to_string(), None))
        .await
        .expect("the turn ended at its deadline, long before the CLI would have answered");

    // Then the turn fails naming its deadline
    let error = ended
        .expect_err("a turn that outlived its deadline is an error")
        .to_string();
    assert!(
        error.contains(
            "claude turn did not end within its 1s deadline; its process group was killed"
        ),
        "the error names the agent and the deadline: {error}"
    );
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
    let (driver, _) = cli.driver("claude", CLAUDE_STREAM_JSON, cli.command());
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
    let (driver, _) = cli.driver("codex", CODEX_JSONL, cli.command());
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
async fn the_model_the_permission_prompt_tool_and_the_withdrawn_tools_reach_the_renderer() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "ok"), 0);
    let (driver, renderer) = cli.driver(
        "claude",
        CLAUDE_STREAM_JSON,
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
    let turns = renderer.turns.lock().expect("turns");
    assert_eq!(
        (
            turns[0].model.as_deref(),
            turns[0].permission_prompt_tool.as_deref(),
            turns[0].denied_tools.clone(),
        ),
        (
            Some("sonnet"),
            Some("mcp__tddy-tools__approval_prompt"),
            vec!["Bash".to_string(), "Edit".to_string()]
        )
    );
}

#[tokio::test]
async fn a_turn_without_a_scratch_directory_is_refused_before_anything_is_spawned() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "ok"), 0);
    let (driver, _) = cli.driver(
        "claude",
        CLAUDE_STREAM_JSON,
        HeadlessCommand {
            scratch_dir: None,
            ..cli.command()
        },
    );

    // When
    let refused = driver.prompt_turn("Go".to_string(), None).await;

    // Then
    assert_eq!(
        (refused.map_err(|e| e.to_string()), cli.invocations()),
        (
            Err("a rendered claude turn writes its registration files to a scratch directory, and the conversation names none".to_string()),
            Vec::<String>::new()
        )
    );
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
    let (driver, _) = cli.driver("oz", OZ_NDJSON, cli.command());

    // When
    let first = a_turn(&driver, "Reply with ok").await;
    a_turn(&driver, "And again").await;

    // Then
    assert_eq!(first.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(first.text, "ok\n");
    assert_eq!(driver.session_id().as_deref(), Some("conv-0001"));
    let calls = cli.invocations();
    assert_eq!(
        calls,
        vec![
            "-p Reply with ok".to_string(),
            "-p And again --resume conv-0001".to_string()
        ],
        "a first turn opens a conversation, the next resumes it"
    );
}

#[tokio::test]
async fn an_oz_turns_commands_and_edits_become_tool_calls_with_their_results() {
    // Given
    let cli = a_fake_cli(&an_oz_capture("tool_turn.ndjson"), 0);
    let (driver, _) = cli.driver("oz", OZ_NDJSON, cli.command());
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
async fn the_worktree_model_tool_server_and_profile_reach_the_renderer_for_oz() {
    // Given
    let cli = a_fake_cli(&an_oz_capture("text_turn.ndjson"), 0);
    let server = McpServerSpec {
        name: "tddy-tools".to_string(),
        command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
        args: vec!["--mcp".to_string()],
        env: Vec::new(),
        cwd: None,
    };
    let (driver, renderer) = cli.driver(
        "oz",
        OZ_NDJSON,
        HeadlessCommand {
            model: Some("claude-sonnet".to_string()),
            mcp_server: Some(server),
            profile: Some("tddy-subagent".to_string()),
            ..cli.command()
        },
    );

    // When
    a_turn(&driver, "Go").await;

    // Then
    let turns = renderer.turns.lock().expect("turns");
    assert_eq!(
        (
            turns[0].workdir.clone(),
            turns[0].model.as_deref(),
            turns[0].mcp_server.clone(),
            turns[0].params.clone(),
        ),
        (
            cli.path().to_path_buf(),
            Some("claude-sonnet"),
            Some(HeadlessMcpServer {
                name: "tddy-tools".to_string(),
                command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
                args: vec!["--mcp".to_string()],
                env: BTreeMap::new(),
                cwd: None,
            }),
            BTreeMap::from([("profile".to_string(), "tddy-subagent".to_string())]),
        )
    );
}

// ---------------------------------------------------------------------------
// Usage tap (`#usage-binding 3/5`): the events a spec declares as usage, appended verbatim
// ---------------------------------------------------------------------------

/// The spec's `usage_events` for a stream-json agent: its `result` events.
fn result_events() -> Vec<tddy_usage::FieldEq> {
    serde_json::from_value(serde_json::json!([{ "field": "type", "equals": "result" }]))
        .expect("a predicate list")
}

#[tokio::test]
async fn a_stream_json_result_is_tapped_verbatim_into_the_conversations_usage_file() {
    // Given — a result carrying the usage and cost a real `claude -p` reports
    let result = serde_json::json!({
        "type": "result", "subtype": "success", "result": "Done.", "session_id": "cl-1",
        "total_cost_usd": 0.579661, "usage": { "input_tokens": 1217, "output_tokens": 40 }
    });
    let mut stream = claude_stream("cl-1", "Done.");
    stream[2] = result.clone();
    let cli = a_fake_cli(&stream, 0);
    let tap = cli
        .path()
        .join("conversation")
        .join("usage")
        .join("conv-1.jsonl");
    let (driver, _) = cli.driver(
        "claude",
        CLAUDE_STREAM_JSON,
        HeadlessCommand {
            usage_tap: Some(tddy_agent_backend::usage_tap::UsageTapSpec {
                path: tap.clone(),
                events: result_events(),
            }),
            ..cli.command()
        },
    );

    // When — two turns
    a_turn(&driver, "Do it").await;
    a_turn(&driver, "Again").await;

    // Then — one tapped line per turn, each the event exactly as the agent wrote it
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&tap)
        .expect("the tap file exists")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(lines, vec![result.clone(), result]);
}

#[tokio::test]
async fn a_turn_without_a_usage_tap_writes_no_usage_file() {
    // Given
    let cli = a_fake_cli(&claude_stream("cl-1", "Done."), 0);
    let (driver, _) = cli.driver("claude", CLAUDE_STREAM_JSON, cli.command());

    // When
    a_turn(&driver, "Do it").await;

    // Then
    assert!(!cli.path().join("conversation").join("usage").exists());
}
