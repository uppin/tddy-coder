//! Acceptance tests: a headless coding agent's permission request, answered through
//! `exec_tools.ExecToolService/RequestApproval`.
//!
//! A conversation's approval is accepted only from the session that owns it: the launch registers
//! the conversation's session and the credential its agent was minted, and a request must present
//! both. The test harness resolves exactly one credential ([`TEST_TOKEN`]), so the caller in the
//! ownership test is that authenticated credential presented against a conversation another
//! credential owns. A parked permission is answered by the conversation's `ParkedPermissions`, and
//! a request nothing answers in time is denied.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::sync::Arc;
use std::time::Duration;

use tddy_coding_subagent::{ApprovalRoutes, ParkedPermissions};
use tddy_core::{PermissionDecision, PermissionHandler, PermissionOptionView, PermissionRequest};
use tddy_daemon_rpc::ExecToolRpcHandler;
use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView};
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{ApprovalRequest, ExecToolService};
use tddy_session_lifecycle::test_util::{test_host, TEST_TOKEN};
use tddy_tool_engine::ExecToolServiceImpl;

const CONVERSATION: &str = "conv-1";
const SESSION: &str = "sess-1";
/// The minted credential the conversation's agent was launched with, when it is not the caller's.
const OWNER_TOKEN: &str = "tok-owner-of-conv-1";
/// How long a test waits for the conversation's own steps before it gives up.
const WAIT: Duration = Duration::from_secs(5);

/// The exec-tool service of a daemon whose approval routes are `routes`.
struct Daemon {
    _sessions: tempfile::TempDir,
    service: ExecToolServiceImpl<ExecToolRpcHandler>,
}

fn a_daemon_with_routes(routes: &Arc<ApprovalRoutes>) -> Daemon {
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let host = test_host(sessions.path().to_path_buf()).with_approval_routes(Arc::clone(routes));
    let handler = ExecToolRpcHandler::from_host(&host);
    Daemon {
        _sessions: sessions,
        service: ExecToolServiceImpl::new(Arc::new(handler)),
    }
}

fn an_approval(
    session_token: &str,
    session_id: &str,
    conversation_id: &str,
) -> Request<ApprovalRequest> {
    Request::direct(ApprovalRequest {
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
        conversation_id: conversation_id.to_string(),
        tool_name: "Bash".to_string(),
        input_json: "{}".to_string(),
        ..Default::default()
    })
}

/// The agent's own permission request, as a headless CLI parks one on its conversation.
fn an_agent_request() -> PermissionRequest {
    PermissionRequest {
        session_id: SESSION.to_string(),
        tool_call_id: "tool-call-1".to_string(),
        title: Some("Edit".to_string()),
        raw_input: None,
        options: vec![
            PermissionOptionView {
                option_id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: "allow_once".to_string(),
            },
            PermissionOptionView {
                option_id: "reject".to_string(),
                name: "Reject".to_string(),
                kind: "reject_once".to_string(),
            },
        ],
    }
}

/// The request parked on `permissions`, once one parks.
async fn the_request_parked_on(permissions: &ParkedPermissions) -> PermissionRequestView {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(view) = permissions.pending() {
                return view;
            }
            permissions.parked().await;
        }
    })
    .await
    .expect("a permission request parks on the conversation")
}

#[tokio::test]
async fn a_tagged_approval_is_answered_by_the_conversations_parked_permission_over_the_rpc() {
    // Given: the conversation is launched by the session that holds the caller's credential, and
    // its agent's permission requests park on its ParkedPermissions.
    let routes = Arc::new(ApprovalRoutes::default().with_timeout(WAIT));
    routes.register_launch(CONVERSATION, SESSION, TEST_TOKEN);
    let permissions = Arc::new(ParkedPermissions::default());
    routes.register_permissions(CONVERSATION, Arc::clone(&permissions));
    let daemon = a_daemon_with_routes(&routes);
    let answering = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move {
            let pending = the_request_parked_on(&permissions).await;
            permissions
                .answer(&PermissionAnswer::Selected {
                    request_id: pending.request_id,
                    option_id: "allow".to_string(),
                })
                .expect("the pending request takes the answer");
        }
    });

    // When: the agent asks for approval of a tool call on its conversation.
    let response = daemon
        .service
        .request_approval(an_approval(TEST_TOKEN, SESSION, CONVERSATION))
        .await;

    // Then: the parked permission's answer is the approval.
    answering.await.expect("the answering task finishes");
    let response = response.expect("the approval is answered").into_inner();
    assert!(
        response.allow,
        "the parked permission was allowed, but the approval said: {}",
        response.message
    );
}

#[tokio::test]
async fn an_approval_is_refused_for_a_token_that_does_not_own_the_conversation() {
    // Given: the conversation was launched with another session's credential, and its agent has a
    // permission request parked, waiting on an answer.
    let routes = Arc::new(ApprovalRoutes::default().with_timeout(WAIT));
    routes.register_launch(CONVERSATION, SESSION, OWNER_TOKEN);
    let permissions = Arc::new(ParkedPermissions::default());
    routes.register_permissions(CONVERSATION, Arc::clone(&permissions));
    let daemon = a_daemon_with_routes(&routes);
    let agent = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move { permissions.decide(an_agent_request()).await }
    });
    let parked = the_request_parked_on(&permissions).await;

    // When: the authenticated caller, which does not own the conversation, asks for approval.
    let refused = daemon
        .service
        .request_approval(an_approval(TEST_TOKEN, SESSION, CONVERSATION))
        .await;

    // Then: the approval is refused, and the parked permission was not decided by it.
    let status = refused.expect_err("a token that does not own the conversation is refused");
    assert_eq!(status.code(), Code::PermissionDenied);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        permissions.pending().map(|view| view.request_id),
        Some(parked.request_id),
        "the refused approval left the agent's request pending"
    );
    assert!(
        !agent.is_finished(),
        "the agent's request was not answered by the refused approval"
    );
    permissions.cancel_pending();
    assert_eq!(
        agent.await.expect("the agent's request finishes"),
        PermissionDecision::Cancelled
    );
}

#[tokio::test]
async fn an_approval_is_denied_when_the_conversation_does_not_answer_in_time() {
    // Given: the conversation is owned by the caller, its permissions are registered, and nothing
    // will answer the request it parks.
    let routes = Arc::new(ApprovalRoutes::default().with_timeout(Duration::from_millis(200)));
    routes.register_launch(CONVERSATION, SESSION, TEST_TOKEN);
    let permissions = Arc::new(ParkedPermissions::default());
    routes.register_permissions(CONVERSATION, Arc::clone(&permissions));
    let daemon = a_daemon_with_routes(&routes);

    // When: the agent asks for approval and the answer never comes.
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        daemon
            .service
            .request_approval(an_approval(TEST_TOKEN, SESSION, CONVERSATION)),
    )
    .await
    .expect("the approval is denied within its timeout")
    .expect("a timed-out approval is an answer, not a refusal")
    .into_inner();

    // Then: the approval is denied, and the request is not left parked.
    assert!(!response.allow, "a request nothing answered is denied");
    assert!(
        permissions.pending().is_none(),
        "the timed-out request was cancelled, not left parked"
    );
}

#[tokio::test]
async fn an_approval_for_an_unknown_conversation_is_refused() {
    // Given: no route is registered for the conversation.
    let routes = Arc::new(ApprovalRoutes::default());
    let daemon = a_daemon_with_routes(&routes);

    // When: the caller asks for approval on a conversation this daemon does not know.
    let refused = daemon
        .service
        .request_approval(an_approval(TEST_TOKEN, SESSION, "conv-9"))
        .await;

    // Then: the approval is refused.
    let status = refused.expect_err("an unknown conversation is refused");
    assert_eq!(status.code(), Code::PermissionDenied);
}
