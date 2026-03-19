//! Configurable response scenarios for the ACP stub agent.

use serde::{Deserialize, Serialize};

/// Scenario configuration for scripted agent responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scenario {
    /// List of response templates. Each prompt consumes the next response.
    #[serde(default)]
    pub responses: Vec<ResponseTemplate>,
}

/// Template for a single prompt response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTemplate {
    /// Text chunks to stream (each becomes an AgentMessageChunk).
    #[serde(default)]
    pub chunks: Vec<String>,

    /// Tool calls to simulate (name and input).
    #[serde(default)]
    pub tool_calls: Vec<ToolCallTemplate>,

    /// Permission requests to simulate. Each triggers a session_request_permission
    /// call to the client; the stub auto-approves with "allow-once".
    #[serde(default)]
    pub permission_requests: Vec<PermissionRequestTemplate>,

    /// Stop reason: "end_turn", "cancelled", "error", etc.
    #[serde(default = "default_stop_reason")]
    pub stop_reason: String,

    /// If true, return an error instead of success.
    #[serde(default)]
    pub error: bool,
}

/// Template for a permission request (agent asks client for permission).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestTemplate {
    pub title: String,
    #[serde(default)]
    pub locations: Vec<String>,
}

fn default_stop_reason() -> String {
    "end_turn".to_string()
}

impl Default for ResponseTemplate {
    fn default() -> Self {
        Self {
            chunks: vec!["Echo response.".to_string()],
            tool_calls: Vec::new(),
            permission_requests: Vec::new(),
            stop_reason: default_stop_reason(),
            error: false,
        }
    }
}

/// Template for a tool call in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallTemplate {
    pub name: String,
    pub input: serde_json::Value,
}
