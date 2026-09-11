use futures_util::StreamExt;
use tddy_service::proto::session_agents_svc::{
    CancelAgentConversationRequest, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ReportAgentConversationStateRequest, StreamSessionAgentsRequest,
};
// The calls below are `SessionAgentService` trait methods on the daemon's family-B surface, so the
// trait must be in scope.
use tddy_service::proto::session_agents_svc::SessionAgentService as _;

use std::sync::Arc;

use super::{ConnectionServiceImpl, DaemonRpcHandler};

/// The coordinate an in-jail agent relays family B at, read from the crate that serves it so this
/// bridge and `tddy-sandbox-runner`'s allowlist cannot disagree about the name.
const SESSION_AGENT_SERVICE: &str = tddy_session_agents::SERVICE_NAME;

impl ConnectionServiceImpl {
    /// The host-side dispatch a sandboxed session's `SessionChannel` relays family B to.
    ///
    /// Named here rather than assembled at each of the three sandboxed-session spawn paths, so a
    /// jail reaches one handler built one way. Public because it is the thing under test in
    /// `in_jail_conversation_acceptance.rs`: five of family B's nine methods are what
    /// `tddy-sandbox-runner`'s relay allowlist permits, and a test that built its own handler would
    /// prove the allowlist against a lookalike rather than against what a real session spawns.
    #[must_use]
    pub fn sandbox_rpc_handler(&self) -> Arc<dyn tddy_sandbox_runner::HostRpcHandler> {
        Arc::new(DaemonRpcHandler {
            conn: self.self_arc(),
        })
    }
}

#[async_trait::async_trait]
impl tddy_sandbox_runner::HostRpcHandler for DaemonRpcHandler {
    async fn handle_rpc(&self, service: &str, method: &str, payload: &[u8]) -> tddy_rpc::RpcResult {
        use prost::Message;
        use tddy_rpc::Request;
        // Only the RPCs the runner forwards ride this bridge; anything else is a wiring bug
        // (the runner's `ToolExecService` would not forward it) and is refused with `not_found`
        // rather than reaching arbitrary `SessionAgentService` surface from inside a jail.
        match (service, method) {
            (SESSION_AGENT_SERVICE, "StreamSessionAgents") => {
                let req = match StreamSessionAgentsRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode StreamSessionAgentsRequest: {e}"
                            )),
                        ));
                    }
                };
                match self
                    .conn
                    .session_agents_service()
                    .stream_session_agents(Request::new(req))
                    .await
                {
                    Ok(resp) => {
                        let mut stream = resp.into_inner();
                        let (tx, rx) = tokio::sync::mpsc::channel(16);
                        tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                let encoded = frame.map(|roster| roster.encode_to_vec());
                                if tx.send(encoded).await.is_err() {
                                    return;
                                }
                            }
                            // The daemon's stream ended cleanly; the receiver observes
                            // end-of-stream when this sender drops.
                        });
                        tddy_rpc::RpcResult::ServerStream(Ok(rx))
                    }
                    Err(status) => tddy_rpc::RpcResult::ServerStream(Err(status)),
                }
            }
            (SESSION_AGENT_SERVICE, "OpenAgentConversation") => {
                let req = match OpenAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode OpenAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self
                    .conn
                    .session_agents_service()
                    .open_agent_conversation(Request::new(req))
                    .await
                {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            (SESSION_AGENT_SERVICE, "PromptAgentConversation") => {
                let req = match PromptAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode PromptAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self
                    .conn
                    .session_agents_service()
                    .prompt_agent_conversation(Request::new(req))
                    .await
                {
                    Ok(resp) => {
                        let mut stream = resp.into_inner();
                        let (tx, rx) = tokio::sync::mpsc::channel(16);
                        tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                let encoded = frame.map(|chunk| chunk.encode_to_vec());
                                if tx.send(encoded).await.is_err() {
                                    return;
                                }
                            }
                        });
                        tddy_rpc::RpcResult::ServerStream(Ok(rx))
                    }
                    Err(status) => tddy_rpc::RpcResult::ServerStream(Err(status)),
                }
            }
            (SESSION_AGENT_SERVICE, "CancelAgentConversation") => {
                let req = match CancelAgentConversationRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode CancelAgentConversationRequest: {e}"
                            )),
                        ));
                    }
                };
                match self
                    .conn
                    .session_agents_service()
                    .cancel_agent_conversation(Request::new(req))
                    .await
                {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            (SESSION_AGENT_SERVICE, "ReportAgentConversationState") => {
                let req = match ReportAgentConversationStateRequest::decode(payload) {
                    Ok(r) => r,
                    Err(e) => {
                        return tddy_rpc::RpcResult::Unary(Err(
                            tddy_rpc::Status::invalid_argument(format!(
                                "decode ReportAgentConversationStateRequest: {e}"
                            )),
                        ));
                    }
                };
                match self
                    .conn
                    .session_agents_service()
                    .report_agent_conversation_state(Request::new(req))
                    .await
                {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            _ => tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "DaemonRpcHandler does not serve {service}/{method}"
            )))),
        }
    }
}
