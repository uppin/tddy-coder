//! Permission server implementing the approval_prompt MCP tool.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

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
    socket_path: Option<PathBuf>,
}

impl PermissionServer {
    pub fn new() -> Self {
        let socket_path = std::env::var_os("TDDY_SOCKET").map(PathBuf::from);
        Self {
            tool_router: Self::tool_router(),
            socket_path,
        }
    }

    /// Decide allow/deny. Bash(tddy-tools *) and mcp__tddy-tools__* are always allowed.
    /// For other tools: route through TDDY_SOCKET to TUI if available, else deny.
    fn decide(&self, tool_name: &str, input: &Value) -> String {
        if tool_name == "Bash" {
            let command = input.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if command.starts_with("tddy-tools") {
                return serde_json::json!({ "behavior": "allow" }).to_string();
            }
        }
        // mcp__tddy-tools__* — our MCP tools, always allow
        if tool_name.starts_with("mcp__tddy-tools__") {
            return serde_json::json!({ "behavior": "allow" }).to_string();
        }
        // Unknown tool: route through TUI if socket available
        if let Some(ref path) = self.socket_path {
            if let Ok(allow) = Self::relay_approve(path, tool_name, input) {
                return serde_json::json!({
                    "behavior": if allow { "allow" } else { "deny" }
                })
                .to_string();
            }
        }
        serde_json::json!({
            "behavior": "deny",
            "message": format!("Permission denied for {} (no TUI socket)", tool_name)
        })
        .to_string()
    }

    #[cfg(unix)]
    fn relay_approve(
        socket_path: &std::path::Path,
        tool_name: &str,
        input: &Value,
    ) -> Result<bool, ()> {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::time::{Duration, Instant};

        let mut stream = UnixStream::connect(socket_path).map_err(|_| ())?;
        stream.set_nonblocking(true).map_err(|_| ())?;

        let req = serde_json::json!({
            "type": "approve",
            "tool_name": tool_name,
            "input": input
        });
        let line = req.to_string();
        stream.write_all(line.as_bytes()).map_err(|_| ())?;
        stream.write_all(b"\n").map_err(|_| ())?;
        stream.flush().map_err(|_| ())?;

        let mut response_line = String::new();
        let mut buf = [0u8; 256];
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match stream.read(&mut buf) {
                Ok(0) => return Err(()),
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]);
                    response_line.push_str(&s);
                    if response_line.contains('\n') {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() > deadline {
                        return Err(());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return Err(()),
            }
        }
        // Protocol: TUI sends exactly one JSON line per response.
        let response_line = response_line.lines().next().unwrap_or("").trim();

        let response: serde_json::Value = serde_json::from_str(response_line).map_err(|_| ())?;
        let decision = response
            .get("decision")
            .and_then(|d| d.as_str())
            .unwrap_or("deny");
        Ok(decision == "allow")
    }

    #[cfg(not(unix))]
    fn relay_approve(
        _socket_path: &std::path::Path,
        _tool_name: &str,
        _input: &Value,
    ) -> Result<bool, ()> {
        Err(())
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
        self.decide(&tool_name, &input)
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
        let result = PermissionServer::new().decide("Bash", &input);
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
        let result = PermissionServer::new().decide("Bash", &input);
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
        let result = PermissionServer::new().decide("Bash", &input);
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
        let result = PermissionServer::new().decide("mcp__tddy-tools__submit", &input);
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
        let result = PermissionServer::new().decide("mcp__tddy-tools__get_schema", &input);
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
        let result = PermissionServer::new().decide("mcp__evil-server__destroy", &input);
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
        let result = PermissionServer::new().decide("Bash", &input);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "deny",
            "arbitrary Bash commands must be denied, got: {}",
            result
        );
    }
}
