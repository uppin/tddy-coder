//! Permission server implementing the approval_prompt MCP tool.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::io::IsTerminal;

/// Parameters for the approval_prompt tool (Claude Code permission-prompt-tool format).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApprovalPromptInput {
    #[schemars(description = "Name of the tool requesting permission")]
    pub tool_name: String,
    #[schemars(description = "Tool input")]
    pub input: Value,
}

/// MCP server that handles permission prompts for Claude Code.
#[derive(Debug, Clone)]
pub struct PermissionServer {
    #[allow(dead_code)] // Used by #[tool_router] macro
    tool_router: ToolRouter<Self>,
}

impl PermissionServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    fn is_tty() -> bool {
        std::io::stdin().is_terminal()
    }

    /// Decide allow/deny. Non-TTY: deny. TTY: would forward via IPC (Phase 2 - deny for now).
    fn decide(tool_name: &str, _input: &Value) -> String {
        if Self::is_tty() {
            serde_json::json!({
                "behavior": "deny",
                "message": format!("Permission denied for {} (TTY IPC not yet implemented)", tool_name)
            })
            .to_string()
        } else {
            serde_json::json!({
                "behavior": "deny",
                "message": format!("Permission denied for {} (non-interactive mode)", tool_name)
            })
            .to_string()
        }
    }
}

impl Default for PermissionServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl PermissionServer {
    #[tool(
        description = "Return allow/deny for tool use. Used by Claude Code --permission-prompt-tool."
    )]
    fn approval_prompt(
        &self,
        Parameters(ApprovalPromptInput { tool_name, input }): Parameters<ApprovalPromptInput>,
    ) -> String {
        Self::decide(&tool_name, &input)
    }
}

impl rmcp::ServerHandler for PermissionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Permission prompt tool for tddy-coder. Denies unexpected tool requests.",
        )
    }
}
