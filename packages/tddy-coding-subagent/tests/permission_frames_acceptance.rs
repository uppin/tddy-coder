//! A permission request's record on the conversation's update stream: pending while a deferred turn waits on
//! it, and completed — naming who answered and with which option — once it is answered.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::path::PathBuf;
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpAgentCommand, AcpToolStatus, AcpUpdate, OpenSession};
use tddy_coding_subagent::{AnsweredBy, CodingAgentSession};
use tddy_discovery::subagent::{
    PermissionAnswer, PromptOutcome, StopReason, SubagentSession, TurnRequest,
};
use tokio::sync::broadcast;

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A coding agent asking one permission, paused on it; returns the session and the request id.
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
        env: vec![(
            "TDDY_ACP_SCENARIO".to_string(),
            scenario.display().to_string(),
        )],
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
    let yielded: PromptOutcome =
        tokio::time::timeout(Duration::from_secs(10), session.prompt("Run the tests"))
            .await
            .expect("in time")
            .expect("the turn yields");
    assert_eq!(yielded.stop_reason, StopReason::PermissionRequested);
    let request_id = yielded.permission_request.expect("the request").request_id;
    (session, request_id, dir)
}

/// The next record of `tool_call_id` in `status` on the stream, read within a bounded wait.
async fn next_record(
    updates: &mut broadcast::Receiver<AcpUpdate>,
    tool_call_id: &str,
    status: AcpToolStatus,
) -> AcpUpdate {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match updates.recv().await {
                Ok(update) => {
                    if let AcpUpdate::ToolCallRecord {
                        id,
                        status: record_status,
                        ..
                    } = &update
                    {
                        if id == tool_call_id && *record_status == status {
                            return update;
                        }
                    }
                }
                Err(e) => panic!("the update stream ended: {e}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {status:?} record of {tool_call_id} arrived"))
}

#[tokio::test]
#[serial]
async fn a_deferred_request_is_recorded_as_pending_and_deferred_to_the_user() {
    // Given
    let (mut session, request_id, _dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    let mut updates = session
        .updates()
        .expect("a coding agent streams its updates");

    // When
    let deferred = request_id.clone();
    let turn = tokio::spawn(async move {
        session
            .take_turn(TurnRequest::resuming().deferring_permission_to_user(deferred))
            .await
    });
    let record = next_record(&mut updates, "permission:perm-1", AcpToolStatus::Pending).await;

    // Then
    let AcpUpdate::ToolCallRecord {
        title,
        raw_input,
        raw_output,
        ..
    } = record
    else {
        panic!("a record of the request: {record:?}");
    };
    assert_eq!(title, "Permission: Run cargo test");
    assert_eq!(raw_output, None);
    let input = raw_input.expect("the request's input");
    assert_eq!(input["requestId"], "perm-1");
    assert_eq!(input["title"], "Run cargo test");
    assert_eq!(input["deferredToUser"], true);

    // Cleanup: the user answers, so the deferred turn ends and the agent is left idle.
    answering
        .answer_as(
            &PermissionAnswer::Selected {
                request_id,
                option_id: "allow-once".to_string(),
            },
            AnsweredBy::User("alice".to_string()),
        )
        .expect("the user's answer is taken");
    turn.await
        .expect("task")
        .expect("the turn continues to its end");
}

#[tokio::test]
#[serial]
async fn the_users_answer_is_recorded_as_completed_naming_the_user_and_the_option() {
    // Given
    let (mut session, request_id, _dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    let mut updates = session
        .updates()
        .expect("a coding agent streams its updates");
    let deferred = request_id.clone();
    let turn = tokio::spawn(async move {
        session
            .take_turn(TurnRequest::resuming().deferring_permission_to_user(deferred))
            .await
    });
    next_record(&mut updates, "permission:perm-1", AcpToolStatus::Pending).await;

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
    let record = next_record(&mut updates, "permission:perm-1", AcpToolStatus::Completed).await;
    let AcpUpdate::ToolCallRecord { raw_output, .. } = record else {
        panic!("a record of the request: {record:?}");
    };
    assert_eq!(
        raw_output,
        Some(serde_json::json!({ "answeredBy": "alice", "optionId": "allow-once" }))
    );
    turn.await
        .expect("task")
        .expect("the turn continues to its end");
}

#[tokio::test]
#[serial]
async fn the_main_agents_answer_to_a_paused_turn_is_recorded_as_naming_the_main_agent() {
    // Given
    let (mut session, request_id, _dir) = a_paused_agent().await;
    let mut updates = session
        .updates()
        .expect("a coding agent streams its updates");

    // When
    let answer = PermissionAnswer::Selected {
        request_id,
        option_id: "allow-once".to_string(),
    };
    session
        .take_turn(TurnRequest::resuming().with_permission_answer(answer))
        .await
        .expect("the main agent's answer continues the turn");

    // Then
    let record = next_record(&mut updates, "permission:perm-1", AcpToolStatus::Completed).await;
    let AcpUpdate::ToolCallRecord { raw_output, .. } = record else {
        panic!("a record of the request: {record:?}");
    };
    assert_eq!(
        raw_output,
        Some(serde_json::json!({ "answeredBy": "main-agent", "optionId": "allow-once" }))
    );
}

#[tokio::test]
#[serial]
async fn a_cancelled_answer_is_recorded_as_completed_with_cancelled_true() {
    // Given
    let (mut session, request_id, _dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    let mut updates = session
        .updates()
        .expect("a coding agent streams its updates");
    let deferred = request_id.clone();
    let turn = tokio::spawn(async move {
        session
            .take_turn(TurnRequest::resuming().deferring_permission_to_user(deferred))
            .await
    });
    next_record(&mut updates, "permission:perm-1", AcpToolStatus::Pending).await;

    // When
    answering
        .answer_as(
            &PermissionAnswer::Cancelled { request_id },
            AnsweredBy::User("alice".to_string()),
        )
        .expect("the user's answer is taken");

    // Then
    let record = next_record(&mut updates, "permission:perm-1", AcpToolStatus::Completed).await;
    let AcpUpdate::ToolCallRecord { raw_output, .. } = record else {
        panic!("a record of the request: {record:?}");
    };
    assert_eq!(
        raw_output,
        Some(serde_json::json!({ "answeredBy": "alice", "cancelled": true }))
    );
    turn.await
        .expect("task")
        .expect("the turn continues to its end");
}
