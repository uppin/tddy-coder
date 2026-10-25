//! Which side of a parked permission request the caller may answer: a request the turn holds for the
//! user is recorded as deferred to the user, and a request the main agent keeps is not.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

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

/// Poll `holds` until it is true, for up to two seconds. A turn that records its deferral does so
/// on its own task, so the test waits for the record rather than sleeping a guessed length.
async fn eventually(mut holds: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if holds() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
#[serial]
async fn a_request_the_turn_holds_for_the_user_is_deferred_to_the_user() {
    // Given a paused request, and a turn that hands it to the user
    let (mut session, request_id, _dir) = a_paused_agent().await;
    let answering = session.permission_answering();
    let deferred_id = request_id.clone();
    let turn = tokio::spawn(async move {
        session
            .take_turn(TurnRequest::resuming().deferring_permission_to_user(deferred_id))
            .await
    });

    // When the turn is waiting on the user
    let recorded = eventually(|| answering.deferred_to_user(&request_id)).await;

    // Then the request reads as deferred to the user
    assert!(
        recorded,
        "the held request is recorded as deferred to the user"
    );

    // And the user's answer ends the turn, so nothing is left waiting
    answering
        .answer_as(
            &PermissionAnswer::Selected {
                request_id,
                option_id: "allow-once".to_string(),
            },
            AnsweredBy::User("alice".to_string()),
        )
        .expect("the user's answer is taken");
    let finished = tokio::time::timeout(Duration::from_secs(10), turn)
        .await
        .expect("the turn continued")
        .expect("task")
        .expect("the turn succeeds");
    assert_eq!(finished.stop_reason, StopReason::EndTurn);
}

#[tokio::test]
#[serial]
async fn a_request_the_main_agent_keeps_is_not_deferred_to_the_user() {
    // Given a paused request the caller has not handed to the user
    let (session, request_id, _dir) = a_paused_agent().await;

    // When the request is asked whether the user may answer it
    let answering = session.permission_answering();

    // Then it is not deferred to the user
    assert!(
        !answering.deferred_to_user(&request_id),
        "a request the main agent keeps is not deferred to the user"
    );
}
