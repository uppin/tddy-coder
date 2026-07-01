//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC
//! or direct HTTP, depending on environment.

use std::path::PathBuf;

pub use tddy_sandbox::session_id_from_env;

/// How `tddy-tools --mcp` reaches the daemon's `ExecuteTool` handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionToolTransport {
    /// In-jail MCP → unix socket → sandbox-runner → SessionChannel → host daemon.
    SandboxIpc { socket_path: PathBuf },
    /// Direct HTTP Connect POST to `ConnectionService/ExecuteTool`.
    DaemonHttp {
        session_id: String,
        daemon_url: String,
        session_token: String,
        daemon_instance_id: String,
    },
}

/// Detect which transport is configured for session tool dispatch.
pub fn detect_session_tool_transport() -> Option<SessionToolTransport> {
    if let Some(socket_path) = std::env::var_os("TDDY_SANDBOX_TOOL_IPC") {
        return Some(SessionToolTransport::SandboxIpc {
            socket_path: PathBuf::from(socket_path),
        });
    }
    let session_id = std::env::var("TDDY_REMOTE_SESSION_ID").ok();
    let daemon_url = std::env::var("TDDY_REMOTE_DAEMON_URL").ok();
    if let (Some(session_id), Some(daemon_url)) = (session_id, daemon_url) {
        return Some(SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token: std::env::var("TDDY_REMOTE_SESSION_TOKEN").unwrap_or_default(),
            daemon_instance_id: std::env::var("TDDY_REMOTE_DAEMON_INSTANCE_ID").unwrap_or_default(),
        });
    }
    None
}

/// Format an MCP tool result string from a daemon `ExecuteToolResponse` body.
///
/// On success returns `result_json` verbatim. On error returns a JSON object with
/// `error` and `is_error: true`.
pub fn format_tool_dispatch_result(body: &serde_json::Value) -> String {
    if body
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        serde_json::json!({
            "error": body
                .get("error_message")
                .and_then(|v| v.as_str())
                .unwrap_or("relay error"),
            "is_error": true
        })
        .to_string()
    } else {
        body.get("result_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}")
            .to_string()
    }
}

fn not_configured_error() -> String {
    serde_json::json!({
        "error": "remote toolset not configured: TDDY_REMOTE_SESSION_ID and TDDY_REMOTE_DAEMON_URL must be set",
        "is_error": true
    })
    .to_string()
}

/// Dispatch a dynamic tool call to the session daemon (sandbox IPC or HTTP).
pub async fn dispatch_session_tool(tool_name: &str, args: serde_json::Value) -> String {
    let Some(transport) = detect_session_tool_transport() else {
        return not_configured_error();
    };
    match transport {
        SessionToolTransport::SandboxIpc { socket_path } => {
            dispatch_via_sandbox_ipc(&socket_path, tool_name, &args).await
        }
        SessionToolTransport::DaemonHttp {
            session_id,
            daemon_url,
            session_token,
            daemon_instance_id,
        } => {
            dispatch_via_daemon_http(
                &daemon_url,
                &session_id,
                &session_token,
                &daemon_instance_id,
                tool_name,
                &args,
            )
            .await
        }
    }
}

/// `dispatch_via_sandbox_ipc`'s stdio-RPC connection never receives inbound calls from the
/// sandbox-runner — any request here would be a bug, so it fails loudly rather than silently
/// no-op'ing.
struct NoCallbackToolService;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for NoCallbackToolService {
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

/// Forward a tool call over the sandbox unix IPC socket, using `tddy-rpc`'s length-prefixed
/// framing (`connection.ConnectionService/ExecuteTool`) rather than the socket path itself
/// carrying any particular wire format — the socket is just a duplex byte stream `tddy-stdio`'s
/// `StdioEndpoint` can wrap like any other (see `StdioEndpoint::from_duplex`).
pub async fn dispatch_via_sandbox_ipc(
    socket_path: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let stream = match tokio::net::UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("tool ipc connect: {e}"), "is_error": true})
                .to_string();
        }
    };
    let (read_half, write_half) = tokio::io::split(stream);
    let (client, endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, NoCallbackToolService);
    tokio::spawn(endpoint.run());
    let client: std::sync::Arc<dyn tddy_rpc::RpcClientTransport> = client;
    dispatch_via_stdio_rpc(&client, tool_name, args).await
}

/// Forward a tool call via HTTP to `ConnectionService/ExecuteTool`.
pub async fn dispatch_via_daemon_http(
    daemon_url: &str,
    session_id: &str,
    session_token: &str,
    daemon_instance_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    let req_body = serde_json::json!({
        "session_token": session_token,
        "session_id": session_id,
        "tool_name": tool_name,
        "args_json": args.to_string(),
        "daemon_instance_id": daemon_instance_id,
    });

    let url = format!(
        "{}/connection.ConnectionService/ExecuteTool",
        daemon_url.trim_end_matches('/')
    );

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(body) => format_tool_dispatch_result(&body),
            Err(e) => {
                serde_json::json!({"error": format!("relay parse error: {e}"), "is_error": true})
                    .to_string()
            }
        },
        Err(e) => {
            serde_json::json!({"error": format!("relay connection error: {e}"), "is_error": true})
                .to_string()
        }
    }
}

/// Forward a tool call over an already-connected RPC transport (e.g. `tddy-stdio`'s
/// `StdioRpcClient`), calling `connection.ConnectionService/ExecuteTool`.
pub async fn dispatch_via_stdio_rpc(
    client: &std::sync::Arc<dyn tddy_rpc::RpcClientTransport>,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use prost::Message;
    use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};

    let request = ExecuteToolRequest {
        session_token: String::new(),
        session_id: String::new(),
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: String::new(),
    };
    let response_bytes = match client
        .call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            request.encode_to_vec(),
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(e) => {
            return serde_json::json!({"error": format!("stdio rpc call: {e}"), "is_error": true})
                .to_string();
        }
    };
    let response = match ExecuteToolResponse::decode(response_bytes.as_slice()) {
        Ok(resp) => resp,
        Err(e) => {
            return serde_json::json!({
                "error": format!("stdio rpc decode response: {e}"),
                "is_error": true
            })
            .to_string();
        }
    };
    if response.is_error {
        serde_json::json!({"error": response.error_message, "is_error": true}).to_string()
    } else {
        response.result_json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tool_dispatch_result_returns_result_json_on_success() {
        // Given
        let body = serde_json::json!({
            "result_json": r#"{"path":"README.md"}"#,
            "is_error": false
        });

        // When
        let out = format_tool_dispatch_result(&body);

        // Then
        assert_eq!(out, r#"{"path":"README.md"}"#);
    }

    #[test]
    fn format_tool_dispatch_result_returns_error_object_on_failure() {
        // Given
        let body = serde_json::json!({
            "is_error": true,
            "error_message": "permission denied"
        });

        // When
        let out = format_tool_dispatch_result(&body);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");

        // Then
        assert_eq!(parsed["is_error"], true);
        assert_eq!(parsed["error"], "permission denied");
    }

    #[test]
    #[serial_test::serial]
    fn detect_transport_prefers_sandbox_ipc_over_remote() {
        // Given
        std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/tmp/tddy-tool-ipc.sock");
        std::env::set_var("TDDY_REMOTE_SESSION_ID", "remote-session");
        std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:8080");

        // When
        let transport = detect_session_tool_transport().expect("transport");

        // Then
        assert_eq!(
            transport,
            SessionToolTransport::SandboxIpc {
                socket_path: PathBuf::from("/tmp/tddy-tool-ipc.sock")
            }
        );

        std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
        std::env::remove_var("TDDY_REMOTE_SESSION_ID");
        std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
    }
}
