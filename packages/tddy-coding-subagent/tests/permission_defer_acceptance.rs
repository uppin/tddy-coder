//! A permission request the main agent handed to the user: the turn waits for the user's answer,
//! and the first answer — the user's or the main agent's — is the one the agent receives.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::path::PathBuf;
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpAgentCommand, OpenSession};
use tddy_coding_subagent::{AnsweredBy, CodingAgentSession};
use tddy_discovery::subagent::{PermissionAnswer, StopReason, SubagentSession, TurnRequest};

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A coding agent asking one permission, paused on it; returns the session, the request id and the
/// agent's record file.
async fn a_paused_agent() -> (CodingAgentSession, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let scenario = dir.path().join("scenario.json");
    std::fs::write(
        &scenario,
        serde_json::json!({ "responses": [{
            "chunks": ["tests pass"], "tool_calls": [],
            "permission_requests": [{ "title": "Run cargo test" }],
        }] })
        .to_string(),
    )
    .expect("scenario");
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
                dir.path().join("record.jsonl").display().to_string(),
            ),
        ],
        cwd: None,
    };
    let mut session = CodingAgentSession::start(
        command,
        OpenSession {
            cwd: dir.path().to_path_buf(),
            ..OpenSession::default()
        },
        "stub-model".to_string(),
    )
    .await
    .expect("start");
    let yielded = tokio::time::timeout(Duration::from_secs(10), session.prompt("Run the tests"))
        .await
        .expect("in time")
        .expect("the turn yields");
    assert_eq!(yielded.stop_reason, StopReason::PermissionRequested);
    let request_id = yielded.permission_request.expect("the request").request_id;
    (session, request_id, dir)
}

fn received_outcomes(dir: &tempfile::TempDir) -> Vec<String> {
    std::fs::read_to_string(dir.path().join("record.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON"))
        .filter(|line| line["event"] == "permission_outcome")
        .map(|line| line["outcome"].as_str().expect("outcome").to_string())
        .collect()
}

#[tokio::test]
#[serial]
async fn a_deferred_request_waits_for_the_users_answer_and_the_turn_then_continues() {
    // Given
    let (mut session, request_id, dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    let deferred_id = request_id.clone();
    let turn = tokio::spawn(async move {
        session
            .take_turn(TurnRequest::resuming().deferring_permission_to_user(deferred_id))
            .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!turn.is_finished(), "the turn waits for the user");

    // When
    answering
        .answer_as(
            &PermissionAnswer::Selected {
                request_id,
                option_id: "allow-once".to_string(),
            },
            AnsweredBy::User("alice".to_string()),
        )
        .expect("the user's answer is taken");

    // Then
    let finished = tokio::time::timeout(Duration::from_secs(10), turn)
        .await
        .expect("the turn continued")
        .expect("task")
        .expect("the turn succeeds");
    assert_eq!(finished.stop_reason, StopReason::EndTurn);
    assert_eq!(received_outcomes(&dir), ["selected:allow-once"]);
}

#[tokio::test]
#[serial]
async fn the_first_answer_wins_and_the_second_is_refused_naming_who_answered() {
    // Given
    let (session, request_id, _dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    answering
        .answer_as(
            &PermissionAnswer::Selected {
                request_id: request_id.clone(),
                option_id: "reject-once".to_string(),
            },
            AnsweredBy::User("alice".to_string()),
        )
        .expect("the first answer");

    // When
    let refusal = answering
        .answer_as(
            &PermissionAnswer::Selected {
                request_id: request_id.clone(),
                option_id: "allow-once".to_string(),
            },
            AnsweredBy::MainAgent,
        )
        .expect_err("a second answer is refused");

    // Then
    assert!(refusal.contains("alice"), "{refusal}");
    assert_eq!(
        answering.answered_by(&request_id),
        Some(AnsweredBy::User("alice".to_string()))
    );
}
