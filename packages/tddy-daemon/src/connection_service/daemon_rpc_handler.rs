use futures_util::StreamExt;
// A `ConnectionService` trait method is called on `self` here, so the trait must be in scope.
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::CancelAgentConversationRequest;

use tddy_service::proto::connection::PromptAgentConversationRequest;

use tddy_service::proto::connection::OpenAgentConversationRequest;


use tddy_service::proto::connection::StreamSessionAgentsRequest;

use super::DaemonRpcHandler;

#[async_trait::async_trait]
impl tddy_sandbox_runner::HostRpcHandler for DaemonRpcHandler {
    async fn handle_rpc(&self, service: &str, method: &str, payload: &[u8]) -> tddy_rpc::RpcResult {
        use prost::Message;
        use tddy_rpc::Request;
        // Only the RPCs the runner forwards ride this bridge; anything else is a wiring bug
        // (the runner's `ToolExecService` would not forward it) and is refused with `not_found`
        // rather than reaching arbitrary `ConnectionService` surface from inside a jail.
        match (service, method) {
            ("connection.ConnectionService", "StreamSessionAgents") => {
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
                match self.conn.stream_session_agents(Request::new(req)).await {
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
            ("connection.ConnectionService", "OpenAgentConversation") => {
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
                match self.conn.open_agent_conversation(Request::new(req)).await {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            ("connection.ConnectionService", "PromptAgentConversation") => {
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
                match self.conn.prompt_agent_conversation(Request::new(req)).await {
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
            ("connection.ConnectionService", "CancelAgentConversation") => {
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
                match self.conn.cancel_agent_conversation(Request::new(req)).await {
                    Ok(resp) => tddy_rpc::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),
                    Err(status) => tddy_rpc::RpcResult::Unary(Err(status)),
                }
            }
            ("connection.ConnectionService", "ReportAgentConversationState") => {
                let req = match tddy_service::proto::connection::ReportAgentConversationStateRequest::decode(payload) {
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
