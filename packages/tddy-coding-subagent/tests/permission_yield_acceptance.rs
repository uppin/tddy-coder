//! A coding subagent's permission request yields the turn to the main agent, and the main agent's
//! answer continues the same agent turn.
//!
//! The stub agent asks through `session/request_permission` and records the outcome it was given
//! (`TDDY_ACP_STUB_RECORD`), so what the agent received is observed, not inferred.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

use std::path::{Path, PathBuf};
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpAgentCommand, OpenSession};
use tddy_coding_subagent::CodingAgentSession;
use tddy_discovery::subagent::{
    PermissionAnswer, PromptOutcome, StopReason, SubagentSession, TurnRequest,
};

const TURN_DEADLINE: Duration = Duration::from_secs(10);

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A coding agent (the stub) whose next turn asks permission for each of `titles`, then says
/// `answer`.
struct AskingAgent {
    dir: tempfile::TempDir,
}

async fn an_agent_asking_for(titles: &[&str], answer: &str) -> (AskingAgent, CodingAgentSession) {
    let agent = AskingAgent {
        dir: tempfile::tempdir().expect("tempdir"),
    };
    let requests: Vec<serde_json::Value> = titles
        .iter()
        .map(|title| serde_json::json!({ "title": title }))
        .collect();
    let scenario = agent.dir.path().join("scenario.json");
    std::fs::write(
        &scenario,
        serde_json::json!({ "responses": [{
            "chunks": [answer],
            "tool_calls": [],
            "permission_requests": requests,
        }] })
        .to_string(),
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
                agent.record_path().display().to_string(),
            ),
        ],
        cwd: None,
    };
    let open = OpenSession {
        cwd: agent.dir.path().to_path_buf(),
        ..OpenSession::default()
    };
    let session = CodingAgentSession::start(command, open, "stub-model".to_string())
        .await
        .expect("the coding agent starts");
    (agent, session)
}

impl AskingAgent {
    fn record_path(&self) -> PathBuf {
        self.dir.path().join("record.jsonl")
    }

    /// The outcomes the agent received for its permission requests, in order.
    fn received_outcomes(&self) -> Vec<String> {
        recorded_events(&self.record_path(), "permission_outcome")
            .into_iter()
            .map(|line| line["outcome"].as_str().expect("outcome").to_string())
            .collect()
    }

    fn recorded(&self, event: &str) -> Vec<serde_json::Value> {
        recorded_events(&self.record_path(), event)
    }

    /// Wait until the agent has recorded `event`, failing past [`TURN_DEADLINE`]. The agent records
    /// a `session/cancel` notification on its own schedule, after the cancelled turn has ended.
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

/// The agent's recorded `event`s. The agent appends to its record while a test reads it, so only
/// complete (newline-ended) lines are read: a trailing fragment is a line still being written.
fn recorded_events(path: &Path, event: &str) -> Vec<serde_json::Value> {
    let record = std::fs::read_to_string(path).unwrap_or_default();
    let complete = record.rfind('\n').map_or("", |end| &record[..end]);
    complete
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("record line is JSON"))
        .filter(|line| line["event"] == event)
        .collect()
}

async fn within_deadline(
    turn: impl std::future::Future<
        Output = Result<PromptOutcome, tddy_discovery::subagent::SubagentError>,
    >,
) -> PromptOutcome {
    tokio::time::timeout(TURN_DEADLINE, turn)
        .await
        .expect("the turn yielded in time")
        .expect("the turn succeeds")
}

fn answering(outcome: &PromptOutcome, option_id: &str) -> TurnRequest {
    let request = outcome
        .permission_request
        .as_ref()
        .expect("the turn yielded a permission request");
    TurnRequest::resuming().with_permission_answer(PermissionAnswer::Selected {
        request_id: request.request_id.clone(),
        option_id: option_id.to_string(),
    })
}

fn text_of(outcome: &PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect()
}

#[tokio::test]
#[serial]
async fn a_permission_request_ends_the_call_with_the_request_and_its_options() {
    // Given
    let (_agent, mut session) = an_agent_asking_for(&["Run cargo test"], "tests pass").await;

    // When
    let outcome = within_deadline(session.prompt("Run the tests")).await;

    // Then
    assert_eq!(outcome.stop_reason, StopReason::PermissionRequested);
    let request = outcome.permission_request.expect("the request");
    assert_eq!(request.tool_call.title, "Run cargo test");
    let options: Vec<(&str, &str)> = request
        .options
        .iter()
        .map(|option| (option.option_id.as_str(), option.kind.as_str()))
        .collect();
    assert_eq!(
        options,
        [("allow-once", "allow_once"), ("reject-once", "reject_once")]
    );
    assert!(!request.request_id.is_empty(), "the request is named");
}

#[tokio::test]
#[serial]
async fn resuming_with_an_option_continues_the_same_turn_and_the_agent_receives_it() {
    // Given
    let (agent, mut session) = an_agent_asking_for(&["Run cargo test"], "tests pass").await;
    let yielded = within_deadline(session.prompt("Run the tests")).await;

    // When
    let finished = within_deadline(session.take_turn(answering(&yielded, "allow-once"))).await;

    // Then
    assert_eq!(agent.received_outcomes(), ["selected:allow-once"]);
    assert_eq!(finished.stop_reason, StopReason::EndTurn);
    assert_eq!(text_of(&finished), "tests pass");
    assert_eq!(
        agent.recorded("new_session").len(),
        1,
        "the same agent session"
    );
}

#[tokio::test]
#[serial]
async fn a_cancelled_answer_reaches_the_agent_as_cancelled() {
    // Given
    let (agent, mut session) = an_agent_asking_for(&["Delete target/"], "skipped").await;
    let yielded = within_deadline(session.prompt("Clean up")).await;
    let request_id = yielded.permission_request.expect("the request").request_id;

    // When
    within_deadline(session.take_turn(
        TurnRequest::resuming().with_permission_answer(PermissionAnswer::Cancelled { request_id }),
    ))
    .await;

    // Then
    assert_eq!(agent.received_outcomes(), ["cancelled"]);
}

#[tokio::test]
#[serial]
async fn a_second_request_in_the_same_turn_yields_again() {
    // Given
    let (agent, mut session) =
        an_agent_asking_for(&["Write src/lib.rs", "Run cargo test"], "done").await;
    let first = within_deadline(session.prompt("Fix and test")).await;

    // When
    let second = within_deadline(session.take_turn(answering(&first, "allow-once"))).await;

    // Then
    assert_eq!(second.stop_reason, StopReason::PermissionRequested);
    let second_request = second
        .permission_request
        .clone()
        .expect("the second request");
    assert_eq!(second_request.tool_call.title, "Run cargo test");
    assert_ne!(
        second_request.request_id,
        first.permission_request.expect("the first").request_id,
        "each request is named on its own"
    );
    let last = within_deadline(session.take_turn(answering(&second, "reject-once"))).await;
    assert_eq!(last.stop_reason, StopReason::EndTurn);
    assert_eq!(
        agent.received_outcomes(),
        ["selected:allow-once", "selected:reject-once"]
    );
}

#[tokio::test]
#[serial]
async fn a_prompt_while_a_request_is_pending_is_refused_naming_it() {
    // Given
    let (_agent, mut session) = an_agent_asking_for(&["Run cargo test"], "tests pass").await;
    let yielded = within_deadline(session.prompt("Run the tests")).await;
    let request_id = yielded.permission_request.expect("the request").request_id;

    // When
    let refusal = session
        .prompt("Something else")
        .await
        .expect_err("a prompt while a request is pending is refused");

    // Then
    assert!(
        refusal.to_string().contains(&request_id),
        "names the pending request {request_id}: {refusal}"
    );
}

#[tokio::test]
#[serial]
async fn a_resume_naming_another_request_is_refused_naming_the_pending_one() {
    // Given
    let (agent, mut session) = an_agent_asking_for(&["Run cargo test"], "tests pass").await;
    let yielded = within_deadline(session.prompt("Run the tests")).await;
    let pending = yielded.permission_request.expect("the request").request_id;

    // When
    let foreign = session
        .take_turn(
            TurnRequest::resuming().with_permission_answer(PermissionAnswer::Selected {
                request_id: "not-the-pending-one".to_string(),
                option_id: "allow-once".to_string(),
            }),
        )
        .await
        .expect_err("a foreign request id is refused");
    let unanswered = session
        .take_turn(TurnRequest::resuming())
        .await
        .expect_err("a resume answering nothing is refused while a request is pending");

    // Then
    assert!(foreign.to_string().contains(&pending), "{foreign}");
    assert!(unanswered.to_string().contains(&pending), "{unanswered}");
    assert!(
        agent.received_outcomes().is_empty(),
        "the agent got no answer"
    );
}

#[tokio::test]
#[serial]
async fn cancelling_while_a_request_is_pending_returns_cancelled() {
    // Given
    let (agent, mut session) = an_agent_asking_for(&["Run cargo test"], "tests pass").await;
    within_deadline(session.prompt("Run the tests")).await;
    let cancel = session.cancel_handle();

    // When
    cancel.cancel().await.expect("cancel is sent");

    // Then
    assert_eq!(agent.received_outcomes(), ["cancelled"]);
    agent.wait_for("cancel").await;
    assert_eq!(agent.recorded("cancel").len(), 1);
}
