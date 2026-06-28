//! Wire protocol between in-jail sandbox-runner and `tddy-tools --mcp` tool dispatch.

use serde::{Deserialize, Serialize};

/// Request sent over `TDDY_SANDBOX_TOOL_IPC` unix socket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIpcRequest {
    pub tool_name: String,
    pub args_json: String,
}

/// Response from the sandbox-runner tool IPC server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIpcResponse {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
}

impl ToolIpcResponse {
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

/// Session id for sandbox or remote MCP tool dispatch.
pub fn session_id_from_env() -> String {
    std::env::var("TDDY_SANDBOX_SESSION_ID")
        .or_else(|_| std::env::var("TDDY_REMOTE_SESSION_ID"))
        .unwrap_or_default()
}
