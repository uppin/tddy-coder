//! Acceptance test: a client that disconnects while its approval is parked does not leave the
//! agent's permission request parked until the conversation closes.
//!
//! A disconnect drops the call in flight. The approval waiting on its conversation's parked request
//! is then dropped with it, and that one request is released as cancelled, so the agent is not left
//! waiting on an answer nobody is waiting to give.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::sync::Arc;
use std::time::Duration;

use tddy_coding_subagent::{ApprovalRoutes, ParkedPermissions};
use tddy_daemon_rpc::ExecToolRpcHandler;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ApprovalRequest, ExecToolService};
use tddy_session_lifecycle::test_util::{test_host, TEST_TOKEN};
use tddy_tool_engine::ExecToolServiceImpl;

const CONVERSATION: &str = "conv-1";
const SESSION: &str = "sess-1";
/// How long the test waits for the conversation's own steps before it gives up.
const WAIT: Duration = Duration::from_secs(5);
/// How long a released request may take to leave the queue.
const RELEASE: Duration = Duration::from_secs(2);

fn an_approval() -> Request<ApprovalRequest> {
    Request::direct(ApprovalRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: SESSION.to_string(),
        conversation_id: CONVERSATION.to_string(),
        tool_name: "Bash".to_string(),
        input_json: "{}".to_string(),
        ..Default::default()
    })
}

/// The request parked on `permissions`, once one parks.
async fn the_request_parked_on(permissions: &ParkedPermissions) {
    tokio::time::timeout(WAIT, async {
        loop {
            if permissions.pending().is_some() {
                return;
            }
            permissions.parked().await;
        }
    })
    .await
    .expect("a permission request parks on the conversation");
}

#[tokio::test]
async fn a_disconnected_clients_parked_approval_is_released_not_left_until_close() {
    // Given: the conversation is owned by the caller, and an approval for it has parked its request.
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let routes = Arc::new(ApprovalRoutes::default().with_timeout(WAIT));
    routes.register_launch(CONVERSATION, SESSION, TEST_TOKEN);
    let permissions = Arc::new(ParkedPermissions::default());
    routes.register_permissions(CONVERSATION, Arc::clone(&permissions));
    let host = test_host(sessions.path().to_path_buf()).with_approval_routes(Arc::clone(&routes));
    let service = Arc::new(ExecToolServiceImpl::new(Arc::new(
        ExecToolRpcHandler::from_host(&host),
    )));
    let asking = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.request_approval(an_approval()).await }
    });
    the_request_parked_on(&permissions).await;

    // When: the client disconnects, which drops the approval still in flight.
    asking.abort();
    assert!(
        matches!(asking.await, Err(error) if error.is_cancelled()),
        "the approval was dropped with its client"
    );

    // Then: the request it parked is released, not left parked until the conversation closes.
    let released = tokio::time::timeout(RELEASE, async {
        while permissions.pending().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "the disconnected client's approval is still parked: {:?}",
        permissions.pending().map(|view| view.request_id)
    );
}
