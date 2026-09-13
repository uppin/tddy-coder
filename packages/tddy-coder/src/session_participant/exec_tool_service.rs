//! The coder's `exec_tools.ExecToolService` coordinate — family L as this process serves it.
//!
//! The coder is the *second* server of the exec-tool family, exactly as
//! [`super::activity_service`] made it the second server of the activity family: a session reached
//! over LiveKit is answered here and the same session reached over HTTP is answered by the daemon.
//! `#unbundle` node 8 moved these four off `the pre-unbundle monolithic RPC coordinate`, so this module is where
//! they answer.

use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;

use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::exec_tools::{
    ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse,
    ListSessionToolCallsRequest, ListSessionToolCallsResponse, ToolCallInfo,
    ToolDef as ProtoToolDef,
};

use super::connection_service_participant::SessionConnectionService;

/// The `exec_tools.ExecToolService` entry the coder's participant registers.
#[must_use]
pub fn coder_exec_tool_entry(svc: Arc<SessionConnectionService>) -> ServiceEntry {
    ServiceEntry {
        name: tddy_tool_engine::EXEC_TOOL_SERVICE,
        service: Arc::new(CoderExecToolRpc { svc }) as Arc<dyn RpcService>,
    }
}

struct CoderExecToolRpc {
    svc: Arc<SessionConnectionService>,
}

#[async_trait]
impl RpcService for CoderExecToolRpc {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        match method {
            "ListExecTools" => {
                if let Err(e) = ListExecToolsRequest::decode(&message.payload[..]) {
                    return RpcResult::Unary(Err(Status::invalid_argument(format!(
                        "decode ListExecToolsRequest: {e}"
                    ))));
                }
                let tools: Vec<ProtoToolDef> = self
                    .svc
                    .list_exec_tools()
                    .into_iter()
                    .map(|t| ProtoToolDef {
                        name: t.name,
                        description: t.description,
                        input_schema_json: t.input_schema_json,
                    })
                    .collect();
                let resp = ListExecToolsResponse { tools };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            "ExecuteTool" => {
                let req = match ExecuteToolRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode ExecuteToolRequest: {e}"
                        ))))
                    }
                };
                let r = self.svc.execute_tool(&req.tool_name, &req.args_json).await;
                let resp = ExecuteToolResponse {
                    result_json: r.result_json,
                    is_error: r.is_error,
                    error_message: r.error_message,
                    job_id: r.job_id,
                    job_running: r.job_running,
                };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            "ListSessionToolCalls" => {
                let req = match ListSessionToolCallsRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode ListSessionToolCallsRequest: {e}"
                        ))))
                    }
                };
                let rows = read_tool_calls(&self.svc.tool_calls_path, &req.session_id);
                let tool_calls: Vec<ToolCallInfo> = rows
                    .into_iter()
                    .map(|r| ToolCallInfo {
                        task_id: r.task_id,
                        tool_name: r.tool_name,
                        args_json: r.args_json,
                        result_json: r.result_json,
                        is_error: r.is_error,
                        error_message: r.error_message,
                        job_running: r.job_running,
                        created_unix_ms: r.created_unix_ms,
                    })
                    .collect();
                let resp = ListSessionToolCallsResponse { tool_calls };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "session participant does not serve ExecToolService/{other}"
            )))),
        }
    }
}

fn read_tool_calls(
    path: &std::path::Path,
    _session_id: &str,
) -> Vec<super::connection_service_participant::ToolCallRecord> {
    use super::connection_service_participant::ToolCallRecord;
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            log::warn!(
                target: "tddy_coder::session_participant::exec_tool_service",
                "read_tool_calls: read {}: {}",
                path.display(),
                e
            );
            return Vec::new();
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| match serde_json::from_str::<ToolCallRecord>(l) {
            Ok(r) => Some(r),
            Err(e) => {
                log::warn!(
                    target: "tddy_coder::session_participant::exec_tool_service",
                    "read_tool_calls: skip malformed line: {}",
                    e
                );
                None
            }
        })
        .collect()
}
