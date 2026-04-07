//! Permission server implementing the approval_prompt MCP tool and GitHub PR REST tools.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tddy_tools::github_pr::{
    create_pull_request_via_rest_api, update_pull_request_via_rest_api, CreatePullRequestParams,
    UpdatePullRequestParams,
};

/// Unix socket for relaying approval prompts to the tddy-coder TUI. In `cfg(test)` builds this is
/// disabled unless `TDDY_TOOLS_TEST_ALLOW_SOCKET=1`, so unit tests never hit a live session when
/// the parent shell leaked `TDDY_SOCKET`.
fn permission_relay_socket_path() -> Option<PathBuf> {
    #[cfg(test)]
    {
        if std::env::var_os("TDDY_TOOLS_TEST_ALLOW_SOCKET").is_some() {
            std::env::var_os("TDDY_SOCKET").map(PathBuf::from)
        } else {
            None
        }
    }
    #[cfg(not(test))]
    {
        std::env::var_os("TDDY_SOCKET").map(PathBuf::from)
    }
}

/// Parameters for the approval_prompt tool (Claude Code permission-prompt-tool format).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApprovalPromptInput {
    #[schemars(description = "Name of the tool requesting permission")]
    pub tool_name: String,
    #[schemars(description = "Tool input")]
    pub input: Value,
}

/// Parameters for [`github_create_pull_request`](PermissionServer::github_create_pull_request).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GithubCreatePullRequestToolInput {
    #[schemars(description = "Repository owner (user or organization).")]
    pub owner: String,
    #[schemars(description = "Repository name.")]
    pub repo: String,
    #[schemars(description = "Pull request title.")]
    pub title: String,
    #[schemars(description = "Head branch name (e.g. feature/foo).")]
    pub head: String,
    #[schemars(description = "Base branch name (e.g. main).")]
    pub base: String,
    #[schemars(description = "Pull request body (description).")]
    pub body: String,
}

/// Parameters for [`github_update_pull_request`](PermissionServer::github_update_pull_request).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GithubUpdatePullRequestToolInput {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Pull request number.")]
    pub pull_number: u64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub draft: Option<bool>,
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
        let socket_path = permission_relay_socket_path();
        Self {
            tool_router: Self::tool_router(),
            socket_path,
        }
    }

    /// Allowed dirs from TDDY_SESSION_DIR and TDDY_REPO_DIR (canonicalized).
    fn allowed_dirs() -> Vec<PathBuf> {
        let session_dir = std::env::var_os("TDDY_SESSION_DIR").map(PathBuf::from);
        let repo_dir = std::env::var_os("TDDY_REPO_DIR").map(PathBuf::from);
        [session_dir, repo_dir]
            .into_iter()
            .flatten()
            .filter_map(|p| std::fs::canonicalize(&p).ok())
            .collect()
    }

    /// True if path (absolute or relative to repo) is under allowed dirs.
    /// For non-existent paths (e.g. mkdir target), walks up to find an existing ancestor.
    fn path_allowed(path: &str) -> bool {
        let allowed = Self::allowed_dirs();
        if allowed.is_empty() {
            return false;
        }
        let path = std::path::Path::new(path);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match allowed.first() {
                Some(base) => base.join(path),
                None => return false,
            }
        };
        let canonical = resolved.canonicalize().ok().or_else(|| {
            let mut current = resolved.as_path();
            while let Some(parent) = current.parent() {
                if let Ok(c) = parent.canonicalize() {
                    return Some(c);
                }
                current = parent;
            }
            None
        });
        canonical
            .map(|c| allowed.iter().any(|a| c.starts_with(a)))
            .unwrap_or(false)
    }

    /// True if all absolute paths in the command are under TDDY_SESSION_DIR or TDDY_REPO_DIR.
    fn paths_in_command_all_allowed(command: &str) -> bool {
        let allowed = Self::allowed_dirs();
        if allowed.is_empty() {
            return false;
        }
        for token in command.split_whitespace() {
            let path_str = token.trim_end_matches(|c: char| "|;&<>".contains(c));
            if path_str.starts_with('/') && !Self::path_allowed(path_str) {
                return false;
            }
        }
        true
    }

    /// True if the tool targets in-repo/plan paths or is a plan/MD submission. Pre-approve when so.
    fn tool_in_repo_pre_allowed(tool_name: &str, input: &Value) -> bool {
        if Self::allowed_dirs().is_empty() {
            return false;
        }
        match tool_name {
            "Write" | "Edit" | "NotebookEdit" => {
                let path = input
                    .get("file_path")
                    .or_else(|| input.get("path"))
                    .and_then(|v| v.as_str());
                match path {
                    Some(p) => Self::path_allowed(p),
                    None => false,
                }
            }
            "ExitPlanMode" | "EnterPlanMode" => {
                // Plan/PRD submission or mode switch — part of tddy workflow
                true
            }
            "AskUserQuestion" => {
                // Clarification flow — part of tddy workflow (matches tddy-core allowlists)
                true
            }
            "Glob" | "Grep" | "Read" => {
                let path = input
                    .get("path")
                    .or_else(|| input.get("directory"))
                    .or_else(|| input.get("glob_pattern"))
                    .and_then(|v| v.as_str());
                path.is_some_and(Self::path_allowed)
            }
            _ => false,
        }
    }

    /// Decide allow/deny. Bash(tddy-tools *) and mcp__tddy-tools__* are always allowed.
    /// Bash commands that only reference paths under TDDY_SESSION_DIR or TDDY_REPO_DIR are pre-allowed.
    /// For other tools: route through TDDY_SOCKET to TUI if available, else deny.
    ///
    /// Claude Code permission-prompt-tool expects allow responses to include `updatedInput` (the
    /// original or modified tool input). Deny responses use `behavior: "deny"` and optional `message`.
    fn decide(&self, tool_name: &str, input: &Value) -> String {
        let allow_response = || serde_json::json!({ "behavior": "allow", "updatedInput": input });
        if tool_name == "Bash" {
            let command = input.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if command.starts_with("tddy-tools") {
                return allow_response().to_string();
            }
            // Pre-allow if all paths in command are under session/plan dir or repo
            if Self::paths_in_command_all_allowed(command) {
                return allow_response().to_string();
            }
        }
        // mcp__tddy-tools__* — our MCP tools, always allow
        if tool_name.starts_with("mcp__tddy-tools__") {
            return allow_response().to_string();
        }
        // In-repo changes, executions, plan/MD submissions — pre-allow when paths are under repo/plan
        if Self::tool_in_repo_pre_allowed(tool_name, input) {
            return allow_response().to_string();
        }
        // Unknown tool: route through TUI if socket available
        if let Some(ref path) = self.socket_path {
            if let Ok(allow) = Self::relay_approve(path, tool_name, input) {
                return if allow {
                    allow_response().to_string()
                } else {
                    serde_json::json!({
                        "behavior": "deny",
                        "message": format!("Permission denied for {}", tool_name)
                    })
                    .to_string()
                };
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

    #[tool(
        description = "Create a GitHub pull request (REST POST /repos/{owner}/{repo}/pulls). Requires GITHUB_TOKEN or GH_TOKEN; uses curl against api.github.com."
    )]
    fn github_create_pull_request(
        &self,
        Parameters(p): Parameters<GithubCreatePullRequestToolInput>,
    ) -> String {
        log::info!(
            target: "tddy_tools::server",
            "MCP github_create_pull_request owner={} repo={}",
            p.owner,
            p.repo
        );
        let params = CreatePullRequestParams {
            owner: p.owner,
            repo: p.repo,
            title: p.title,
            head: p.head,
            base: p.base,
            body: p.body,
        };
        match create_pull_request_via_rest_api(&params) {
            Ok(n) => {
                log::debug!(
                    target: "tddy_tools::server",
                    "github_create_pull_request: created pull_number={}",
                    n
                );
                serde_json::json!({ "pull_number": n }).to_string()
            }
            Err(e) => {
                let msg = format!("{e}");
                log::debug!(
                    target: "tddy_tools::server",
                    "github_create_pull_request: error {}",
                    msg
                );
                serde_json::json!({ "error": msg }).to_string()
            }
        }
    }

    #[tool(
        description = "Update an existing GitHub pull request metadata (REST PATCH). Requires GITHUB_TOKEN or GH_TOKEN."
    )]
    fn github_update_pull_request(
        &self,
        Parameters(p): Parameters<GithubUpdatePullRequestToolInput>,
    ) -> String {
        log::info!(
            target: "tddy_tools::server",
            "MCP github_update_pull_request owner={} repo={} pull_number={}",
            p.owner,
            p.repo,
            p.pull_number
        );
        let params = UpdatePullRequestParams {
            owner: p.owner,
            repo: p.repo,
            pull_number: p.pull_number,
            title: p.title,
            body: p.body,
            draft: p.draft,
        };
        match update_pull_request_via_rest_api(&params) {
            Ok(()) => {
                log::debug!(
                    target: "tddy_tools::server",
                    "github_update_pull_request: success pr={}",
                    params.pull_number
                );
                serde_json::json!({ "ok": true }).to_string()
            }
            Err(e) => {
                let msg = format!("{e}");
                log::debug!(
                    target: "tddy_tools::server",
                    "github_update_pull_request: error {}",
                    msg
                );
                serde_json::json!({ "error": msg }).to_string()
            }
        }
    }
}

#[tool_handler]
impl rmcp::ServerHandler for PermissionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Permission prompt tool for tddy-coder. Denies unexpected tool requests. \
             When **GITHUB_TOKEN** or **GH_TOKEN** is set, this server also exposes GitHub PR tools: \
             **github_create_pull_request** and **github_update_pull_request** (REST via curl to api.github.com).",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;
    use serial_test::serial;

    #[test]
    fn mcp_server_get_info_mentions_github_pr_tools() {
        let info = PermissionServer::new().get_info();
        let text = info
            .instructions
            .as_deref()
            .expect("server instructions must be set");
        assert!(
            text.contains("github_create_pull_request")
                && text.contains("github_update_pull_request"),
            "MCP server instructions must name GitHub PR tools; got: {text}"
        );
    }

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
    #[serial]
    fn approval_prompt_pre_allows_paths_in_repo_dir() {
        let dir = std::env::temp_dir().join("tddy-preallow-test");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let subdir = repo.join("packages").join("tddy-core");
        std::fs::create_dir_all(&subdir).unwrap();
        let subdir = std::fs::canonicalize(&subdir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);
        let result = {
            let input = serde_json::json!({
                "command": format!("ls -la {} | grep -E '\\.rs$'", subdir.display())
            });
            PermissionServer::new().decide("Bash", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash with path in TDDY_REPO_DIR must be pre-allowed, got: {}",
            result
        );
    }

    #[test]
    #[serial]
    fn approval_prompt_pre_allows_mkdir_for_nonexistent_path_in_repo() {
        let dir = std::env::temp_dir().join("tddy-mkdir-preallow");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let packages = repo.join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        let mkdir_target = repo.join("packages").join("tddy-github").join("src");

        std::env::set_var("TDDY_REPO_DIR", &repo);
        let result = {
            let input = serde_json::json!({
                "command": format!("mkdir -p {}", mkdir_target.display())
            });
            PermissionServer::new().decide("Bash", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash mkdir -p for path under TDDY_REPO_DIR must be pre-allowed (path may not exist yet), got: {}",
            result
        );
    }

    #[test]
    #[serial]
    fn approval_prompt_pre_allows_write_in_repo_dir() {
        let dir = std::env::temp_dir().join("tddy-write-preallow");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let file_path = repo.join("src").join("lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);
        let result = {
            let input = serde_json::json!({
                "file_path": file_path.display().to_string(),
                "content": "// test"
            });
            PermissionServer::new().decide("Write", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Write with path in TDDY_REPO_DIR must be pre-allowed, got: {}",
            result
        );
    }

    #[test]
    #[serial]
    fn approval_prompt_pre_allows_exit_plan_mode() {
        let dir = std::env::temp_dir().join("tddy-exitplan");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);
        let result = {
            let input = serde_json::json!({
                "plan": "# PRD\n\n## Summary\nTest",
                "allowedPrompts": []
            });
            PermissionServer::new().decide("ExitPlanMode", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "ExitPlanMode must be pre-allowed when TDDY env set, got: {}",
            result
        );
        assert!(
            parsed.get("updatedInput").is_some(),
            "Claude Code permission-prompt-tool expects updatedInput in allow responses, got: {}",
            result
        );
    }

    #[test]
    #[serial]
    fn approval_prompt_pre_allows_ask_user_question() {
        let dir = std::env::temp_dir().join("tddy-askuser");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);
        let result = {
            let input = serde_json::json!({
                "questions": [{"header": "Scope", "question": "Which?", "options": [], "multiSelect": false}]
            });
            PermissionServer::new().decide("AskUserQuestion", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "AskUserQuestion must be pre-allowed when TDDY env set, got: {}",
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
