//! Client for the toolcall relay socket (`TDDY_SOCKET`), served by `tddy-core`'s
//! `ToolcallRpcService` over `tddy-rpc`/`tddy-stdio` framing instead of the old bespoke
//! newline-delimited-JSON protocol. The wire *payloads* (the same `"type"`-discriminated JSON
//! objects the old protocol used) are unchanged — only the framing/dispatch that carries them
//! changed, mirroring the sandbox tool-IPC migration (see `session_tool_client`).

use std::path::Path;

use tddy_rpc::RpcClientTransport;

/// The relay never initiates calls into `tddy-tools` over this connection — any inbound request
/// here would be a bug, so it fails loudly rather than silently no-op'ing.
struct NoCallbackToolcallService;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for NoCallbackToolcallService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unimplemented(format!(
            "tddy-tools hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// Map the wire request's `"type"` discriminator to the RPC method name
/// `tddy_core::toolcall::ToolcallRpcService` dispatches on.
fn method_for(request: &serde_json::Value) -> Result<&'static str, String> {
    let req_type = request
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "toolcall request is missing its 'type' field".to_string())?;
    Ok(match req_type {
        "submit" => "Submit",
        "ask" => "Ask",
        "approve" => "Approve",
        "list-actions" => "ListActions",
        "invoke-action" => "InvokeAction",
        "build" => "Build",
        "build-list" => "BuildList",
        "spawn-child" => "SpawnChild",
        "spawn-conversation" => "SpawnConversation",
        other => return Err(format!("unknown toolcall request type: {other}")),
    })
}

/// Relay `request` (the same `"type"`-discriminated wire object the old protocol used) to the
/// toolcall listener at `socket_path`, over `tddy-rpc`/`tddy-stdio` framing, and parse its JSON
/// response.
pub async fn dispatch_toolcall(
    socket_path: &Path,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let method = method_for(&request)?;

    let stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|e| format!("failed to connect to TDDY_SOCKET: {e}"))?;
    let (read_half, write_half) = tokio::io::split(stream);
    let (client, endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, NoCallbackToolcallService);
    tokio::spawn(endpoint.run());

    let payload = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    let response_bytes = client
        .call_unary("tddy.toolcall.ToolcallService", method, payload)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&response_bytes).map_err(|e| format!("invalid response JSON: {e}"))
}
