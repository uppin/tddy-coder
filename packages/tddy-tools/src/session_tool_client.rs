//! Generic session tool dispatch — forwards MCP tool calls to `tddy-daemon` via sandbox IPC
//! or direct HTTP, depending on environment.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tddy_service::proto::connection::ExecuteToolResponse;

/// Wire format for sandbox tool IPC (`TDDY_SANDBOX_TOOL_IPC` unix socket).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIpcRequest {
    pub tool_name: String,
    pub args_json: String,
}

/// Wire format returned by the sandbox-runner tool IPC server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIpcResponse {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
}

impl ToolIpcResponse {
    pub fn from_execute_tool(resp: &ExecuteToolResponse) -> Self {
        Self {
            result_json: resp.result_json.clone(),
            is_error: resp.is_error,
            error_message: resp.error_message.clone(),
        }
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            serde_json::json!({
                "error": "failed to encode tool ipc response",
                "is_error": true
            })
            .to_string()
        })
    }
}

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

/// Resolve session id from sandbox or remote env (used when building ExecuteToolRequest).
pub fn session_id_from_env() -> String {
    std::env::var("TDDY_SANDBOX_SESSION_ID")
        .or_else(|_| std::env::var("TDDY_REMOTE_SESSION_ID"))
        .unwrap_or_default()
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

/// Forward a tool call over the sandbox unix IPC socket.
pub async fn dispatch_via_sandbox_ipc(
    socket_path: &std::path::Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = match tokio::net::UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("tool ipc connect: {e}"), "is_error": true})
                .to_string();
        }
    };
    let req = ToolIpcRequest {
        tool_name: tool_name.to_string(),
        args_json: args.to_string(),
    };
    let payload = match serde_json::to_string(&req) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({
                "error": format!("tool ipc encode request: {e}"),
                "is_error": true
            })
            .to_string();
        }
    };
    if stream.write_all(payload.as_bytes()).await.is_err() {
        return serde_json::json!({"error": "tool ipc write failed", "is_error": true}).to_string();
    }
    let _ = stream.shutdown().await;
    let mut buf = vec![0u8; 65536];
    match stream.read(&mut buf).await {
        Ok(n) if n > 0 => {
            let raw = String::from_utf8_lossy(&buf[..n]);
            match serde_json::from_str::<ToolIpcResponse>(&raw) {
                Ok(resp) if resp.is_error => serde_json::json!({
                    "error": resp.error_message,
                    "is_error": true
                })
                .to_string(),
                Ok(resp) => resp.result_json,
                Err(_) => raw.to_string(),
            }
        }
        Ok(_) => {
            serde_json::json!({"error": "tool ipc empty response", "is_error": true}).to_string()
        }
        Err(e) => serde_json::json!({"error": format!("tool ipc read: {e}"), "is_error": true})
            .to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_ipc_response_round_trips_json() {
        // Given
        let resp = ToolIpcResponse {
            result_json: r#"{"ok":true}"#.to_string(),
            is_error: false,
            error_message: String::new(),
        };

        // When
        let json = resp.to_json_string();
        let parsed: ToolIpcResponse = serde_json::from_str(&json).expect("parse");

        // Then
        assert_eq!(parsed, resp);
    }

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
