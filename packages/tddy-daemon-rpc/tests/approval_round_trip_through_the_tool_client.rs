//! Round trip: a headless agent's approval asked through the tool client reaches the daemon's
//! `RequestApproval` and is answered by the conversation's parked permission.
//!
//! The client is `tddy-session-tool-client`'s `request_approval_via_rpc_transport`, which takes an
//! `RpcClientTransport`. That transport here is the daemon's own RPC dispatch, in-process: the
//! bridge a socket or LiveKit connection hands its frames to, with the wire left out. The daemon is
//! the test service the other daemon RPC tests serve.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tddy_coding_subagent::{ApprovalRoutes, ParkedPermissions};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView};
use tddy_rpc::{
    MultiRpcService, RequestMetadata, RequestTransport, ResponseBody, RpcBridge,
    RpcClientTransport, RpcMessage, Status,
};
use tddy_session_lifecycle::test_util::{test_host, TEST_TOKEN};
use tddy_session_tool_client::{request_approval_via_rpc_transport, SessionToolEnvelope};
use tddy_tool_engine::build_exec_tool_entry;

const CONVERSATION: &str = "conv-1";
const SESSION: &str = "sess-1";
/// The minted credential the conversation's agent was launched with, when it is not the caller's.
const OWNER_TOKEN: &str = "tok-owner-of-conv-1";
/// How long the test waits for the conversation's own steps before it gives up.
const WAIT: Duration = Duration::from_secs(5);

/// The daemon's exec-tool service, reached over the RPC dispatch every transport ends in. An
/// approval is one unary call, so the other calls are refused.
struct InProcessDaemon {
    bridge: RpcBridge<MultiRpcService>,
}

impl InProcessDaemon {
    fn serving(daemon: TestDaemon) -> Arc<dyn RpcClientTransport> {
        let bridge = RpcBridge::new(MultiRpcService::new(vec![build_exec_tool_entry(daemon)]));
        Arc::new(Self { bridge })
    }
}

#[async_trait]
impl RpcClientTransport for InProcessDaemon {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let message = RpcMessage {
            payload: request_bytes,
            metadata: RequestMetadata::over(RequestTransport::InProcess),
        };
        match self
            .bridge
            .handle_messages(service, method, std::slice::from_ref(&message))
            .await?
        {
            ResponseBody::Complete(frames) => frames
                .into_iter()
                .next()
                .ok_or_else(|| Status::internal("the daemon answered with no reply")),
            ResponseBody::Streaming(_) => {
                Err(Status::internal("an approval is answered as one reply"))
            }
        }
    }

    async fn call_server_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }
}

/// The envelope the tool client sends with, for a caller presenting `session_token`.
fn an_envelope(session_token: &str) -> SessionToolEnvelope {
    SessionToolEnvelope {
        session_token: session_token.to_string(),
        session_id: SESSION.to_string(),
        daemon_instance_id: String::new(),
    }
}

/// A daemon whose conversation `CONVERSATION` is owned by `owner` and parks on `permissions`.
fn a_daemon_owning_the_conversation(
    owner: &str,
    permissions: &Arc<ParkedPermissions>,
) -> (tempfile::TempDir, Arc<dyn RpcClientTransport>) {
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let routes = Arc::new(ApprovalRoutes::default().with_timeout(WAIT));
    routes.register_launch(CONVERSATION, SESSION, owner);
    routes.register_permissions(CONVERSATION, Arc::clone(permissions));
    let host = test_host(sessions.path().to_path_buf()).with_approval_routes(routes);
    (
        sessions,
        InProcessDaemon::serving(TestDaemon::from_host(host)),
    )
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
async fn a_tool_clients_approval_is_answered_by_the_conversations_parked_permission() {
    // Given: the conversation is owned by the caller's session, and its agent's permission parks
    // on the conversation's permissions, to be answered by the conversation.
    let permissions = Arc::new(ParkedPermissions::default());
    let (_sessions, client) = a_daemon_owning_the_conversation(TEST_TOKEN, &permissions);
    let answering = tokio::spawn({
        let permissions = Arc::clone(&permissions);
        async move {
            let parked = the_request_parked_on(&permissions).await;
            permissions
                .answer(&PermissionAnswer::Selected {
                    request_id: parked.request_id,
                    option_id: "allow".to_string(),
                })
                .expect("the parked request takes the answer");
        }
    });

    // When: the tool client asks for approval of a Bash command over the daemon's route.
    let answer = request_approval_via_rpc_transport(
        &client,
        &an_envelope(TEST_TOKEN),
        CONVERSATION,
        "Bash",
        &json!({"command": "ls"}),
    )
    .await;

    // Then: the conversation's answer is the approval.
    answering.await.expect("the answering task finishes");
    assert_eq!(
        answer,
        Ok(true),
        "the conversation's allow reaches the client"
    );
}

#[tokio::test]
async fn a_tool_clients_approval_is_refused_for_a_token_that_does_not_own_the_conversation() {
    // Given: the conversation was launched with another session's credential, so the caller is not
    // the session that owns it, though its own credential resolves on this daemon.
    let permissions = Arc::new(ParkedPermissions::default());
    let (_sessions, client) = a_daemon_owning_the_conversation(OWNER_TOKEN, &permissions);

    // When: the tool client asks for approval over the daemon's route.
    let answer = request_approval_via_rpc_transport(
        &client,
        &an_envelope(TEST_TOKEN),
        CONVERSATION,
        "Bash",
        &json!({"command": "ls"}),
    )
    .await;

    // Then: the client is refused, not allowed, and the refusal says why.
    let reason = answer.expect_err("a caller that does not own the conversation is refused");
    assert!(
        reason.contains("does not belong"),
        "the refusal names the ownership failure: {reason}"
    );
}
