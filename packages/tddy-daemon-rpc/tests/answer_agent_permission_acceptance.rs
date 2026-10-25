//! `AnswerAgentPermission` — the web's answer to a permission request the main agent deferred to
//! the user. A new write path from the browser, so who may call it, and on what, is pinned first.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session_agents_svc::{
    AnswerAgentPermissionRequest, SessionAgentService as _,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

fn an_answer(token: &str, conversation_id: &str) -> AnswerAgentPermissionRequest {
    AnswerAgentPermissionRequest {
        session_token: token.to_string(),
        session_id: "1780828020298-answers".to_string(),
        daemon_instance_id: String::new(),
        conversation_id: conversation_id.to_string(),
        request_id: "perm-1".to_string(),
        option_id: "allow-once".to_string(),
        cancelled: false,
    }
}

#[tokio::test]
async fn answer_agent_permission_is_refused_for_an_unauthenticated_caller() {
    // Given
    let sessions = tempfile::tempdir().expect("sessions");
    let service = test_service(sessions.path().to_path_buf());

    // When
    let refusal = service
        .session_agents_service()
        .answer_agent_permission(Request::direct(an_answer("not-a-token", "conv-1")))
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refusal.code, Code::Unauthenticated);
}

#[tokio::test]
async fn answer_agent_permission_is_refused_for_a_conversation_the_session_does_not_hold() {
    // Given
    let sessions = tempfile::tempdir().expect("sessions");
    let service = test_service(sessions.path().to_path_buf());

    // When
    let refusal = service
        .session_agents_service()
        .answer_agent_permission(Request::direct(an_answer(
            TEST_TOKEN,
            "conv-of-another-session",
        )))
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refusal.code, Code::NotFound);
    assert!(
        refusal.message.contains("conv-of-another-session"),
        "{}",
        refusal.message
    );
}
