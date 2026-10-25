//! Acceptance tests for `CodingAgentSession` — a coding agent's turns, over ACP, as a subagent
//! conversation.
//!
//! Every test drives a real `tddy-acp-stub` child through the ACP driver; the stub records what it
//! was asked (`TDDY_ACP_STUB_RECORD`).
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::path::{Path, PathBuf};
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpAgentCommand, OpenSession};
use tddy_coding_subagent::CodingAgentSession;
use tddy_discovery::subagent::{StopReason, SubagentSession, TurnRequest};

const TURN_DEADLINE: Duration = Duration::from_secs(10);

/// `tddy-acp-stub` in the target directory cargo builds into: `CARGO_TARGET_DIR` when set (a
/// relative one read from the workspace root), else the workspace's `target/`, as cargo itself does.
fn stub_agent_path() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A coding agent (the stub) scripted with `responses`, recording into its own directory.
struct ScriptedAgent {
    dir: tempfile::TempDir,
    responses: Vec<serde_json::Value>,
}

fn a_coding_agent() -> ScriptedAgent {
    ScriptedAgent {
        dir: tempfile::tempdir().expect("tempdir"),
        responses: Vec::new(),
    }
}

fn a_turn() -> serde_json::Value {
    serde_json::json!({ "chunks": [], "tool_calls": [], "permission_requests": [] })
}

impl ScriptedAgent {
    fn answering(mut self, response: serde_json::Value) -> Self {
        self.responses.push(response);
        self
    }

    fn record_path(&self) -> PathBuf {
        self.dir.path().join("record.jsonl")
    }

    async fn start(&self) -> CodingAgentSession {
        let scenario = self.dir.path().join("scenario.json");
        std::fs::write(
            &scenario,
            serde_json::json!({ "responses": self.responses }).to_string(),
        )
        .expect("write scenario");
        let command = AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: vec![
                (
                    "TDDY_ACP_SCENARIO".to_string(),
                    scenario.display().to_string(),
                ),
                (
                    "TDDY_ACP_STUB_RECORD".to_string(),
                    self.record_path().display().to_string(),
                ),
            ],
            cwd: None,
        };
        let open = OpenSession {
            cwd: self.dir.path().to_path_buf(),
            ..OpenSession::default()
        };
        CodingAgentSession::start(command, open, "stub-model".to_string())
            .await
            .expect("the coding agent starts")
    }

    fn recorded(&self, event: &str) -> Vec<serde_json::Value> {
        recorded_events(&self.record_path(), event)
    }

    /// Wait until the agent has recorded `event`, failing past [`TURN_DEADLINE`].
    async fn wait_for(&self, event: &str) {
        tokio::time::timeout(TURN_DEADLINE, async {
            while self.recorded(event).is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the agent recorded `{event}` in time"));
    }
}

fn recorded_events(path: &Path, event: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("record line is JSON"))
        .filter(|line| line["event"] == event)
        .collect()
}

fn text_of(content: &[tddy_discovery::subagent::ContentBlock]) -> String {
    content.iter().map(|block| block.text.as_str()).collect()
}

fn process_is_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[tokio::test]
#[serial]
async fn a_prompt_turn_returns_the_coding_agents_text_as_end_turn() {
    // Given
    let mut answer = a_turn();
    answer["chunks"] = serde_json::json!(["The bug is in ", "parse()."]);
    let agent = a_coding_agent().answering(answer);
    let mut session = agent.start().await;

    // When
    let outcome = tokio::time::timeout(TURN_DEADLINE, session.prompt("Find the bug"))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds");

    // Then
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert_eq!(text_of(&outcome.content), "The bug is in parse().");
}

#[tokio::test]
#[serial]
async fn each_tool_call_of_the_turn_is_one_message_descriptor() {
    // Given
    let mut answer = a_turn();
    answer["tool_calls"] = serde_json::json!([
        { "name": "Read", "input": { "path": "src/lib.rs" } },
        { "name": "Edit", "input": { "path": "src/lib.rs" } },
    ]);
    let agent = a_coding_agent().answering(answer);
    let mut session = agent.start().await;

    // When
    let outcome = tokio::time::timeout(TURN_DEADLINE, session.prompt("Fix it"))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds");

    // Then
    let tools: Vec<Option<&str>> = outcome
        .messages
        .iter()
        .filter(|message| message.tool.is_some())
        .map(|message| message.tool.as_deref())
        .collect();
    assert_eq!(tools, [Some("Read"), Some("Edit")]);
}

/// Take `request` on a fresh session and return its refusal, asserting nothing was re-opened.
async fn refusal_of(request: TurnRequest) -> String {
    let agent = a_coding_agent();
    let mut session = agent.start().await;

    let refusal = session
        .take_turn(request)
        .await
        .expect_err("a loop-only option is refused")
        .to_string();

    assert_eq!(agent.recorded("new_session").len(), 1, "nothing re-opened");
    refusal
}

#[tokio::test]
#[serial]
async fn max_turns_is_refused_naming_it() {
    // Given
    let request = TurnRequest::prompting("Fix the parser").within_turns(3);

    // When
    let refusal = refusal_of(request).await;

    // Then
    assert!(refusal.contains("maxTurns"), "{refusal}");
}

#[tokio::test]
#[serial]
async fn yield_conditions_are_refused_naming_them() {
    // Given
    let yield_on_shell: tddy_discovery::subagent::YieldCondition =
        serde_json::from_value(serde_json::json!({
            "tool": "SHELL",
            "when": { "argument": { "field": "command", "contains": "rm" } },
        }))
        .expect("a yield condition");
    let request =
        TurnRequest::prompting("Fix the parser").with_yield_conditions(vec![yield_on_shell]);

    // When
    let refusal = refusal_of(request).await;

    // Then
    assert!(refusal.contains("yieldConditions"), "{refusal}");
}

#[tokio::test]
#[serial]
async fn a_rewind_point_is_refused_naming_it() {
    // Given
    let request = TurnRequest::resuming().from_message("m-1".into());

    // When
    let refusal = refusal_of(request).await;

    // Then
    assert!(refusal.contains("fromMessageId"), "{refusal}");
}

#[tokio::test]
#[serial]
async fn a_correction_is_refused_naming_it() {
    // Given
    let request = TurnRequest::resuming().with_correction("try again");

    // When
    let refusal = refusal_of(request).await;

    // Then
    assert!(refusal.contains("correction"), "{refusal}");
}

#[tokio::test]
#[serial]
async fn a_replacement_is_refused_naming_it() {
    // Given
    let request = TurnRequest::resuming().with_replacement(
        serde_json::from_value(serde_json::json!({
            "tool": "READ", "arguments": {}, "result": "{}",
        }))
        .expect("a replacement"),
    );

    // When
    let refusal = refusal_of(request).await;

    // Then
    assert!(refusal.contains("replacement"), "{refusal}");
}

#[tokio::test]
#[serial]
async fn a_permission_request_is_denied_and_recorded_in_the_turns_messages() {
    // Given
    let mut asking = a_turn();
    asking["permission_requests"] = serde_json::json!([{ "title": "Run cargo test" }]);
    let agent = a_coding_agent().answering(asking);
    let mut session = agent.start().await;

    // When
    let outcome = tokio::time::timeout(TURN_DEADLINE, session.prompt("Test it"))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds");

    // Then
    let outcome_line = agent.recorded("permission_outcome");
    assert_eq!(outcome_line.len(), 1);
    assert_eq!(outcome_line[0]["outcome"], "selected:reject-once");
    let denial = outcome
        .messages
        .iter()
        .find(|message| message.preview.contains("Run cargo test"))
        .expect("the denial is one of the turn's messages");
    assert!(
        denial.is_error,
        "a denied request reads as an error: {denial:?}"
    );
}

#[tokio::test]
#[serial]
async fn cancelling_a_running_turn_returns_cancelled() {
    // Given
    // The turn asks one permission before it holds: its recorded outcome is the sign the turn is
    // running inside the agent.
    let mut held = a_turn();
    held["permission_requests"] = serde_json::json!([{ "title": "Run cargo test" }]);
    held["hold_until_cancel"] = serde_json::json!(true);
    let agent = a_coding_agent().answering(held);
    let mut session = agent.start().await;
    // Taken before the turn, as the conversation table does: `subagent_cancel` arrives while
    // `subagent_prompt` holds the session.
    let cancel = session.cancel_handle();
    let turn = tokio::spawn(async move { session.prompt("Work").await });
    agent.wait_for("permission_outcome").await;

    // When
    cancel.cancel().await.expect("cancel is sent");

    // Then
    let outcome = tokio::time::timeout(TURN_DEADLINE, turn)
        .await
        .expect("the turn ended after cancel")
        .expect("turn task")
        .expect("a cancelled turn is an outcome, not an error");
    assert_eq!(outcome.stop_reason, StopReason::Cancelled);
    assert_eq!(agent.recorded("cancel").len(), 1);
}

#[tokio::test]
#[serial]
async fn dropping_the_session_stops_the_agent_process() {
    // Given
    let agent = a_coding_agent();
    let session = agent.start().await;
    let pid = session.agent_pid().expect("the agent runs");

    // When
    drop(session);

    // Then
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while process_is_running(pid) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_is_running(pid), "the agent process {pid} is gone");
}
