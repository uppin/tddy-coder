//! Permission server implementing the approval_prompt MCP tool.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;

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

    /// Decide allow/deny. Bash(tddy-tools *) is always allowed for headless permission handling.
    /// All other tool requests are denied.
    fn decide(tool_name: &str, input: &Value) -> String {
        if tool_name == "Bash" {
            let command = input
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if command.starts_with("tddy-tools") {
                return serde_json::json!({ "behavior": "allow" }).to_string();
            }
        }
        serde_json::json!({
            "behavior": "deny",
            "message": format!("Permission denied for {} (not a tddy-tools command)", tool_name)
        })
        .to_string()
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

#[tool_handler]
impl rmcp::ServerHandler for PermissionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Permission prompt tool for tddy-coder. Denies unexpected tool requests.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_submit() {
        let input = serde_json::json!({
            "command": "tddy-tools submit --goal plan --data '{\"goal\":\"plan\",\"prd\":\"# PRD\"}'"
        });
        let result = PermissionServer::decide("Bash", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools submit) must be allowed for headless permission handling, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_ask() {
        let input = serde_json::json!({
            "command": "tddy-tools ask --data '{\"questions\":[]}'"
        });
        let result = PermissionServer::decide("Bash", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools ask) must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_get_schema() {
        let input = serde_json::json!({
            "command": "tddy-tools get-schema plan"
        });
        let result = PermissionServer::decide("Bash", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools get-schema) must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_mcp_tddy_tools_tool_calls() {
        let input = serde_json::json!({
            "goal": "plan",
            "data": "{}"
        });
        let result = PermissionServer::decide("mcp__tddy-tools__submit", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "mcp__tddy-tools__* tool calls must be allowed (it's our tool), got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_mcp_tddy_tools_get_schema() {
        let input = serde_json::json!({
            "goal": "plan"
        });
        let result = PermissionServer::decide("mcp__tddy-tools__get_schema", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "mcp__tddy-tools__get_schema must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_denies_mcp_from_unknown_server() {
        let input = serde_json::json!({ "query": "drop tables" });
        let result = PermissionServer::decide("mcp__evil-server__destroy", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "deny",
            "MCP tools from unknown servers must be denied, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_denies_arbitrary_bash_commands() {
        let input = serde_json::json!({
            "command": "rm -rf /important/data"
        });
        let result = PermissionServer::decide("Bash", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "deny",
            "arbitrary Bash commands must be denied, got: {}",
            result
        );
    }
}
