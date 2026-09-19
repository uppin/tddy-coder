use futures_util::StreamExt;
use tddy_service::proto::session_agents_svc::{
    CancelAgentConversationRequest, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ReportAgentConversationStateRequest, StreamSessionAgentsRequest,
};
// The calls below are `SessionAgentService` trait methods on the daemon's family-B surface, so the
// trait must be in scope.
use tddy_service::proto::session_agents_svc::SessionAgentService as _;

use super::DaemonRpcHandler;

/// The coordinate an in-jail agent relays family B at, read from the crate that serves it so this
/// bridge and `tddy-sandbox-runner`'s allowlist cannot disagree about the name.
const SESSION_AGENT_SERVICE: &str = tddy_session_agents::SERVICE_NAME;

#[async_trait::async_trait]
impl tddy_sandbox_runner::HostRpcHandler for DaemonRpcHandler {
    async fn handle_rpc(&self, service: &str, method: &str, payload: &[u8]) -> tddy_rpc::RpcResult {
        use prost::Message;
        use tddy_rpc::Request;
        // The bridge holds the daemon weakly (a strong reference would be a cycle through the
        // host's own `sandbox_rpc_bridge` field), so the host is resolved once per call. A host
        // that is gone is a refusal, never a silent empty answer — and the variant it is wrapped
        // in does not matter: the relay turns `Unary(Err)` and `ServerStream(Err)` into the same
        // terminal `RpcStreamFrame` with `error` set (`host_relay.rs`).
        let conn = match self.host() {
            Ok(conn) => conn,
            Err(status) => return tddy_rpc::RpcResult::Unary(Err(status)),
        };
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
                match conn
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
                match conn
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
                match conn
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
                match conn
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
                match conn
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
