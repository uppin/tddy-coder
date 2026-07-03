//! Permission server implementing the approval_prompt MCP tool and GitHub PR REST tools.

use crate::github_pr::{
    create_pull_request_via_rest_api, update_pull_request_via_rest_api, CreatePullRequestParams,
    UpdatePullRequestParams,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tddy_discovery::agent_def::SpecializedAgentDef;
use tddy_discovery::subagent::{
    resolve_replaced_tools_for_defs, CodebaseAccess, PromptOutcome, SubagentConfig,
    SubagentRegistry, SubagentSession,
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
    tool_router: ToolRouter<Self>,
    socket_path: Option<PathBuf>,
}

impl PermissionServer {
    pub fn new() -> Self {
        let socket_path = permission_relay_socket_path();
        let mut tool_router = Self::tool_router();
        // Session-tool transport (sandbox IPC or daemon HTTP) present => forward the
        // dynamic exec-tool catalog too, so Claude Code sees Read/Write/Shell/etc.
        // alongside the 3 static tools. Both transport variants use the same static
        // catalog today (see exec_tool_catalog doc comment for why).
        if crate::session_tool_client::detect_session_tool_transport().is_some() {
            // Server-side enforcement of subagent tool replacement: a tool a configured subagent
            // declares it `replaces` is delegated to that subagent, so this server must not
            // advertise it — a direct call must be impossible at the tool server too, not only
            // gated by Claude's allow/disallow lists. Empty when no subagent replaces anything.
            let replaced = resolve_replaced_tools_for_defs(&subagents_from_env());
            let catalog: Vec<RemoteToolDef> = exec_tool_catalog()
                .into_iter()
                .filter(|tool| !replaced.contains(&tool.name))
                .collect();
            tool_router.merge(dynamic_tool_router(&catalog));
        }
        // Discovery-subagent tools (ACP-shaped: subagent_new_session/prompt/cancel) — merged only
        // when a subagent is actually configured, mirroring the exec-tool merge above.
        if subagent_enabled() {
            tool_router.merge(subagent_tool_router());
        }
        Self {
            tool_router,
            socket_path,
        }
    }

    /// Every tool name currently registered in this server's live `ToolRouter` — the exact
    /// set `tools/list` will report, including any merged-in dynamic exec tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
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

// Explicit `router = self.tool_router` — the default `#[tool_handler]` expansion calls the
// static `Self::tool_router()` (the macro-generated router only), which would silently ignore
// any dynamic tools merged into the instance's `self.tool_router` field by `PermissionServer::new()`.
#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for PermissionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Permission prompt tool for tddy-coder. Denies unexpected tool requests. \
             When **GITHUB_TOKEN** or **GH_TOKEN** is set, this server also exposes GitHub PR tools: \
             **github_create_pull_request** and **github_update_pull_request** (REST via curl to api.github.com).",
        )
    }
}

// --- Remote-codebase mode: dynamic tool catalog helpers ---

/// A tool definition fetched from the relay daemon (or configured statically for testing).
pub struct RemoteToolDef {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
}

/// Returns the names of tools that are always statically registered and never forwarded to a relay.
pub fn static_tool_names() -> Vec<&'static str> {
    vec!["approval_prompt", "submit"]
}

/// Build the full MCP tool list: static tools + dynamically-discovered remote tools from `catalog`.
///
/// Static tools (`approval_prompt`, `submit`) are always included first. Dynamic tools are
/// appended in catalog order. If `input_schema_json` is not a valid JSON object, the tool is
/// skipped and an error is returned.
pub async fn build_dynamic_tool_list(
    catalog: &[RemoteToolDef],
) -> anyhow::Result<Vec<rmcp::model::Tool>> {
    let mut tools = vec![];

    // Static tools — always present.
    tools.push(rmcp::model::Tool::new(
        "approval_prompt",
        "Permission approval prompt for tddy-coder.",
        std::sync::Arc::new(serde_json::Map::new()),
    ));
    tools.push(rmcp::model::Tool::new(
        "submit",
        "Submit structured workflow output.",
        std::sync::Arc::new(serde_json::Map::new()),
    ));

    // Dynamic tools from catalog.
    for def in catalog {
        let schema_value: serde_json::Value = serde_json::from_str(&def.input_schema_json)
            .map_err(|e| {
                anyhow::anyhow!(
                    "RemoteToolDef '{}': invalid input_schema_json: {}",
                    def.name,
                    e
                )
            })?;
        let schema_map = schema_value.as_object().cloned().unwrap_or_default();
        tools.push(rmcp::model::Tool::new(
            def.name.clone(),
            def.description.clone(),
            std::sync::Arc::new(schema_map),
        ));
    }

    Ok(tools)
}

/// Returns true if `tool_name` is a native mutation tool that must be hard-denied
/// when the agent is running in remote mode (TDDY_REMOTE_SESSION_ID is set).
///
/// In remote mode the working dir is read-only; native write tools would corrupt it.
pub fn is_native_tool_denied_in_remote_mode(tool_name: &str) -> bool {
    matches!(tool_name, "Write" | "Edit" | "NotebookEdit")
}

/// Dispatch a call to a dynamic (non-static) tool via the session daemon.
///
/// Uses [`crate::session_tool_client::dispatch_session_tool`] — sandbox IPC when
/// `TDDY_SANDBOX_TOOL_IPC` is set, otherwise HTTP to `TDDY_REMOTE_DAEMON_URL`.
pub async fn dispatch_dynamic_tool(tool_name: &str, args: serde_json::Value) -> String {
    crate::session_tool_client::dispatch_session_tool(tool_name, args).await
}

/// Static catalog of the "cursor" exec tools forwarded to Claude Code when a session-tool
/// transport (sandbox IPC or daemon HTTP) is configured. Names/descriptions/schemas mirror
/// `tddy_daemon::tool_catalog::tool_catalog()` verbatim (adapted from `ToolDef` to
/// `RemoteToolDef`) — the two must never drift; `exec_tool_catalog_names_match_workspace_exec_tool_names`
/// and `tddy_daemon`'s own `workspace_exec_tool_names_match_tool_catalog` test both guard this.
///
/// TODO: both transport variants currently use this same static catalog rather than live-fetching
/// the catalog from the daemon over the transport (there is no such message type over
/// SandboxIpc, and it was deliberately scoped out for DaemonHttp too for now).
pub fn exec_tool_catalog() -> Vec<RemoteToolDef> {
    vec![
        RemoteToolDef {
            name: "Read".to_string(),
            description: "Read file contents from the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Write".to_string(),
            description: "Write file contents to the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path","contents"],"properties":{"path":{"type":"string"},"contents":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "StrReplace".to_string(),
            description: "Replace a string in a file.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path","old_string","new_string"],"properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Delete".to_string(),
            description: "Delete a file from the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Grep".to_string(),
            description: "Search for a pattern in files.".to_string(),
            input_schema_json: r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string"},"path":{"type":"string"},"include":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            input_schema_json: r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Shell".to_string(),
            description: "Run a shell command in the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string"},"block_until_ms":{"type":"integer"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "Await".to_string(),
            description: "Wait for a background shell job to complete.".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"job_id":{"type":"string"},"task_id":{"type":"string"},"timeout_ms":{"type":"integer"},"block_until_ms":{"type":"integer"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "ReadLints".to_string(),
            description: "Read linting diagnostics for the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#.to_string(),
        },
        RemoteToolDef {
            name: "SemanticSearch".to_string(),
            description: "Search the codebase semantically.".to_string(),
            input_schema_json: r#"{"type":"object","required":["query"],"properties":{"query":{"type":"string"},"path":{"type":"string"}}}"#.to_string(),
        },
    ]
}

/// Build a live `ToolRouter<PermissionServer>` from an arbitrary catalog of [`RemoteToolDef`]s.
///
/// Pure: reads no env vars, special-cases nothing. Each catalog entry becomes one dynamically
/// dispatched `ToolRoute` whose handler forwards the call name + arguments to
/// [`dispatch_dynamic_tool`] and converts the resulting JSON string into a `CallToolResult`.
pub fn dynamic_tool_router(
    catalog: &[RemoteToolDef],
) -> rmcp::handler::server::router::tool::ToolRouter<PermissionServer> {
    use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};

    let mut router = ToolRouter::new();
    for def in catalog {
        let schema_value: serde_json::Value = serde_json::from_str(&def.input_schema_json)
            .unwrap_or_else(|e| {
                panic!(
                    "RemoteToolDef '{}': invalid input_schema_json: {}",
                    def.name, e
                )
            });
        let schema_map = schema_value.as_object().cloned().unwrap_or_else(|| {
            panic!(
                "RemoteToolDef '{}': input_schema_json must be a JSON object",
                def.name
            )
        });
        let tool = rmcp::model::Tool::new(
            def.name.clone(),
            def.description.clone(),
            std::sync::Arc::new(schema_map),
        );
        let route = ToolRoute::new_dyn(tool, move |ctx| {
            let tool_name = ctx.name().to_string();
            let arguments = serde_json::Value::Object(ctx.arguments.clone().unwrap_or_default());
            Box::pin(async move {
                let result_string = dispatch_dynamic_tool(&tool_name, arguments).await;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(result_string),
                ]))
            })
        });
        router.add_route(route);
    }
    router
}

// --- Discovery subagent MCP tools (ACP-shaped: session/new, session/prompt, session/cancel) ---

/// True when a discovery subagent is configured for this process (`TDDY_SUBAGENT` non-empty).
fn subagent_enabled() -> bool {
    env_non_empty("TDDY_SUBAGENT").is_some()
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

type SubagentSessionTable = tokio::sync::Mutex<HashMap<String, Box<dyn SubagentSession>>>;

/// Process-wide session table — `PermissionServer` merges the subagent router at construction
/// time, but the conversation must survive across separate `tools/call` invocations, so the table
/// lives outside any single `PermissionServer` instance.
fn subagent_sessions() -> &'static SubagentSessionTable {
    static SESSIONS: OnceLock<SubagentSessionTable> = OnceLock::new();
    SESSIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

/// Resolve how a subagent's internal READ/GLOB/GREP calls reach the codebase: explicit
/// `TDDY_SUBAGENT_CODEBASE_ACCESS` override, else `Managed` when a session-tool transport is
/// configured (mirrors the exec-tool gating above), else `Local`.
fn subagent_codebase_access_from_env() -> CodebaseAccess {
    match env_non_empty("TDDY_SUBAGENT_CODEBASE_ACCESS").as_deref() {
        Some("local") => CodebaseAccess::Local,
        Some("managed") => managed_codebase_access(),
        _ => {
            if crate::session_tool_client::detect_session_tool_transport().is_some() {
                managed_codebase_access()
            } else {
                CodebaseAccess::Local
            }
        }
    }
}

/// Wrap [`crate::session_tool_client::dispatch_session_tool`] as a `CodebaseAccess::Managed`
/// dispatch fn — the same proxy transport the exec-tool catalog already uses.
fn managed_codebase_access() -> CodebaseAccess {
    CodebaseAccess::managed(|tool_name: String, args: serde_json::Value| {
        Box::pin(async move {
            crate::session_tool_client::dispatch_session_tool(&tool_name, args).await
        })
    })
}

/// Parse `TDDY_SUBAGENTS_JSON` (a JSON array of [`SpecializedAgentDef`] — see
/// docs/ft/coder/specialized-subagents.md) into the resolved specialized-agent defs for this
/// process. Empty (unset, blank, or unparseable) when the env var is absent — the caller falls
/// back to the legacy single-fastcontext `SubagentRegistry::new()` path in that case, preserving
/// today's `TDDY_SUBAGENT=fastcontext` + `TDDY_SUBAGENT_FASTCONTEXT_*` behavior unchanged.
fn subagents_from_env() -> Vec<SpecializedAgentDef> {
    env_non_empty("TDDY_SUBAGENTS_JSON")
        .and_then(|json| serde_json::from_str::<Vec<SpecializedAgentDef>>(&json).ok())
        .unwrap_or_default()
}

/// Build a [`SubagentConfig`] from `TDDY_SUBAGENT_FASTCONTEXT_*` env vars, with defaults matching
/// `tddy-coder`'s `--fastcontext-*` CLI flags (see docs/ft/coder/discovery-agent.md). Only
/// `access` is meaningful when the registry was built via [`subagents_from_env`]'s defs (the def
/// itself supplies base_url/model/max_turns in that case — see
/// `SubagentRegistry::create`'s doc comment in `tddy-discovery`).
fn subagent_config_from_env() -> SubagentConfig {
    SubagentConfig {
        base_url: env_non_empty("TDDY_SUBAGENT_FASTCONTEXT_URL")
            .unwrap_or_else(|| "http://localhost:30000".to_string()),
        model: env_non_empty("TDDY_SUBAGENT_FASTCONTEXT_MODEL")
            .unwrap_or_else(|| "microsoft/FastContext-1.0-4B-RL".to_string()),
        max_turns: env_non_empty("TDDY_SUBAGENT_FASTCONTEXT_MAX_TURNS")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(10),
        access: subagent_codebase_access_from_env(),
    }
}

fn subagent_error_json(message: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": message.to_string(), "is_error": true }).to_string()
}

fn prompt_outcome_json(outcome: PromptOutcome) -> String {
    serde_json::json!({
        "stopReason": outcome.stop_reason,
        "content": outcome.content,
    })
    .to_string()
}

/// `subagent_new_session` (ACP `session/new`-shaped): opens a conversation with the named
/// subagent (default: `TDDY_SUBAGENT`) under the given `sessionId` — the caller decides the
/// conversation id; one is generated only when omitted.
async fn subagent_new_session_tool(args: serde_json::Value) -> String {
    let agent_name = args
        .get("agent")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| env_non_empty("TDDY_SUBAGENT"));
    let Some(agent_name) = agent_name else {
        return subagent_error_json("no subagent configured: set TDDY_SUBAGENT or pass 'agent'");
    };
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let defs = subagents_from_env();
    let registry = if defs.is_empty() {
        SubagentRegistry::new()
    } else {
        SubagentRegistry::from_defs(defs)
    };
    match registry.create(&agent_name, subagent_config_from_env()) {
        Ok(session) => {
            subagent_sessions()
                .lock()
                .await
                .insert(session_id.clone(), session);
            serde_json::json!({ "sessionId": session_id }).to_string()
        }
        Err(e) => subagent_error_json(e),
    }
}

/// `subagent_prompt` (ACP `session/prompt`-shaped): sends one prompt turn to an already-open
/// session and returns `{stopReason, content}` once the subagent yields.
async fn subagent_prompt_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let Some(prompt_blocks) = args.get("prompt").and_then(|v| v.as_array()) else {
        return subagent_error_json("missing required field: prompt");
    };
    let prompt_text = prompt_blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    if prompt_text.is_empty() {
        return subagent_error_json("prompt must contain at least one non-empty text block");
    }

    let mut sessions = subagent_sessions().lock().await;
    let Some(session) = sessions.get_mut(session_id) else {
        return subagent_error_json(format!("unknown subagent session: {session_id}"));
    };
    match session.prompt(&prompt_text).await {
        Ok(outcome) => prompt_outcome_json(outcome),
        Err(e) => subagent_error_json(e),
    }
}

/// `subagent_cancel` (ACP `session/cancel`-shaped): closes an open session, if any.
async fn subagent_cancel_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let cancelled = subagent_sessions()
        .lock()
        .await
        .remove(session_id)
        .is_some();
    serde_json::json!({ "cancelled": cancelled }).to_string()
}

fn schema_object(
    json: serde_json::Value,
) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    std::sync::Arc::new(json.as_object().cloned().unwrap_or_default())
}

/// Wraps a subagent tool handler (`async fn(Value) -> String`) into a `ToolRoute` — the same
/// success-envelope-with-embedded-error convention `dynamic_tool_router` uses for exec tools.
fn subagent_route<F>(
    tool: rmcp::model::Tool,
    handler: F,
) -> rmcp::handler::server::router::tool::ToolRoute<PermissionServer>
where
    F: Fn(serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send>>
        + Send
        + Sync
        + 'static,
{
    rmcp::handler::server::router::tool::ToolRoute::new_dyn(tool, move |ctx| {
        let arguments = serde_json::Value::Object(ctx.arguments.clone().unwrap_or_default());
        let result_future = handler(arguments);
        Box::pin(async move {
            let result_string = result_future.await;
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text(result_string),
            ]))
        })
    })
}

/// Build the `ToolRouter` for the three ACP-shaped subagent tools. Merged into
/// `PermissionServer::new()`'s router only when [`subagent_enabled`].
fn subagent_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<PermissionServer> {
    use rmcp::handler::server::router::tool::ToolRouter;

    let mut router = ToolRouter::new();

    let new_session_tool = rmcp::model::Tool::new(
        "subagent_new_session",
        "Open a new conversation with a discovery subagent (ACP session/new-shaped). \
         Returns {sessionId}.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {"type": "string", "description": "Subagent name, e.g. 'fastcontext'. Defaults to TDDY_SUBAGENT."},
                "sessionId": {"type": "string", "description": "Caller-chosen conversation id. Generated if omitted."},
                "cwd": {"type": "string", "description": "Optional working directory hint."}
            }
        })),
    );
    router.add_route(subagent_route(new_session_tool, |args| {
        Box::pin(subagent_new_session_tool(args))
    }));

    let prompt_tool = rmcp::model::Tool::new(
        "subagent_prompt",
        "Send a prompt turn to an open subagent session (ACP session/prompt-shaped). \
         Returns {stopReason, content}.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId", "prompt"],
            "properties": {
                "sessionId": {"type": "string"},
                "prompt": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["type", "text"],
                        "properties": {
                            "type": {"type": "string"},
                            "text": {"type": "string"}
                        }
                    }
                }
            }
        })),
    );
    router.add_route(subagent_route(prompt_tool, |args| {
        Box::pin(subagent_prompt_tool(args))
    }));

    let cancel_tool = rmcp::model::Tool::new(
        "subagent_cancel",
        "Close an open subagent session (ACP session/cancel-shaped).",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId"],
            "properties": {
                "sessionId": {"type": "string"}
            }
        })),
    );
    router.add_route(subagent_route(cancel_tool, |args| {
        Box::pin(subagent_cancel_tool(args))
    }));

    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;
    use serial_test::serial;

    #[test]
    fn mcp_server_get_info_mentions_github_pr_tools() {
        // When
        let info = PermissionServer::new().get_info();
        let text = info
            .instructions
            .as_deref()
            .expect("server instructions must be set");

        // Then
        assert!(
            text.contains("github_create_pull_request"),
            "MCP server instructions must name github_create_pull_request; got: {text}"
        );
        assert!(
            text.contains("github_update_pull_request"),
            "MCP server instructions must name github_update_pull_request; got: {text}"
        );
    }

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_submit() {
        // Given
        let input = serde_json::json!({
            "command": "tddy-tools submit --goal plan --data '{\"goal\":\"plan\",\"prd\":\"# PRD\"}'"
        });

        // When
        let result = PermissionServer::new().decide("Bash", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools submit) must be allowed for headless permission handling, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_ask() {
        // Given
        let input = serde_json::json!({
            "command": "tddy-tools ask --data '{\"questions\":[]}'"
        });

        // When
        let result = PermissionServer::new().decide("Bash", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools ask) must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_bash_tddy_tools_get_schema() {
        // Given
        let input = serde_json::json!({
            "command": "tddy-tools get-schema plan"
        });

        // When
        let result = PermissionServer::new().decide("Bash", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "Bash(tddy-tools get-schema) must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_mcp_tddy_tools_tool_calls() {
        // Given
        let input = serde_json::json!({
            "goal": "plan",
            "data": "{}"
        });

        // When
        let result = PermissionServer::new().decide("mcp__tddy-tools__submit", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "mcp__tddy-tools__* tool calls must be allowed (it's our tool), got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_allows_mcp_tddy_tools_get_schema() {
        // Given
        let input = serde_json::json!({
            "goal": "plan"
        });

        // When
        let result = PermissionServer::new().decide("mcp__tddy-tools__get_schema", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "mcp__tddy-tools__get_schema must be allowed, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_denies_mcp_from_unknown_server() {
        // Given
        let input = serde_json::json!({ "query": "drop tables" });

        // When
        let result = PermissionServer::new().decide("mcp__evil-server__destroy", &input);

        // Then
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
        // Given
        let dir = std::env::temp_dir().join("tddy-preallow-test");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let subdir = repo.join("packages").join("tddy-core");
        std::fs::create_dir_all(&subdir).unwrap();
        let subdir = std::fs::canonicalize(&subdir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);

        // When
        let result = {
            let input = serde_json::json!({
                "command": format!("ls -la {} | grep -E '\\.rs$'", subdir.display())
            });
            PermissionServer::new().decide("Bash", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        // Then
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
        // Given
        let dir = std::env::temp_dir().join("tddy-mkdir-preallow");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let packages = repo.join("packages");
        std::fs::create_dir_all(&packages).unwrap();
        let mkdir_target = repo.join("packages").join("tddy-github").join("src");

        std::env::set_var("TDDY_REPO_DIR", &repo);

        // When
        let result = {
            let input = serde_json::json!({
                "command": format!("mkdir -p {}", mkdir_target.display())
            });
            PermissionServer::new().decide("Bash", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        // Then
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
        // Given
        let dir = std::env::temp_dir().join("tddy-write-preallow");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();
        let file_path = repo.join("src").join("lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);

        // When
        let result = {
            let input = serde_json::json!({
                "file_path": file_path.display().to_string(),
                "content": "// test"
            });
            PermissionServer::new().decide("Write", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        // Then
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
        // Given
        let dir = std::env::temp_dir().join("tddy-exitplan");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);

        // When
        let result = {
            let input = serde_json::json!({
                "plan": "# PRD\n\n## Summary\nTest",
                "allowedPrompts": []
            });
            PermissionServer::new().decide("ExitPlanMode", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        // Then
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
        // Given
        let dir = std::env::temp_dir().join("tddy-askuser");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::fs::canonicalize(&dir).unwrap();

        std::env::set_var("TDDY_REPO_DIR", &repo);

        // When
        let result = {
            let input = serde_json::json!({
                "questions": [{"header": "Scope", "question": "Which?", "options": [], "multiSelect": false}]
            });
            PermissionServer::new().decide("AskUserQuestion", &input)
        };
        std::env::remove_var("TDDY_REPO_DIR");
        std::fs::remove_dir_all(&dir).ok();

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "allow",
            "AskUserQuestion must be pre-allowed when TDDY env set, got: {}",
            result
        );
    }

    #[test]
    fn approval_prompt_denies_arbitrary_bash_commands() {
        // Given
        let input = serde_json::json!({
            "command": "rm -rf /important/data"
        });

        // When
        let result = PermissionServer::new().decide("Bash", &input);

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed["behavior"], "deny",
            "arbitrary Bash commands must be denied, got: {}",
            result
        );
    }

    // ─── server-side enforcement of subagent tool replacement ───────────────────────
    //
    // Claude's --allowedTools/--disallowedTools gate the main agent, but the MCP server itself
    // still advertised the full exec catalog. A tool a subagent `replaces` must be unreachable at
    // the server too: the server must not advertise it, so no client can invoke it directly.

    const IPC_SOCKET_ENV: &str = "TDDY_SANDBOX_TOOL_IPC";

    /// Set the env that makes the server advertise exec tools (a session-tool transport) and wire a
    /// single subagent whose `replaces` set is `replaced`, run `f`, then restore the env. Serial
    /// tests only — these vars are process-global.
    fn with_subagent_replacing<R>(replaced: &[&str], f: impl FnOnce() -> R) -> R {
        let defs = format!(
            r#"[{{"name":"fastcontext","model":"m","replaces":[{}]}}]"#,
            replaced
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        std::env::set_var(IPC_SOCKET_ENV, "/tmp/tddy-test-ipc.sock");
        std::env::set_var("TDDY_SUBAGENT", "fastcontext");
        std::env::set_var("TDDY_SUBAGENTS_JSON", defs);
        let result = f();
        std::env::remove_var(IPC_SOCKET_ENV);
        std::env::remove_var("TDDY_SUBAGENT");
        std::env::remove_var("TDDY_SUBAGENTS_JSON");
        result
    }

    /// A tool a subagent declares it `replaces` must not appear in the MCP server's advertised tool
    /// list — the server refuses to serve it, so a direct call is impossible regardless of Claude's
    /// own allow/disallow lists.
    #[test]
    #[serial]
    fn mcp_server_omits_replaced_exec_tools_from_its_advertised_catalog() {
        // Given / When
        let tools = with_subagent_replacing(&["Grep", "Glob", "SemanticSearch"], || {
            PermissionServer::new().tool_names()
        });

        // Then
        for replaced in ["Grep", "Glob", "SemanticSearch"] {
            assert!(
                !tools.contains(&replaced.to_string()),
                "replaced tool {replaced} must not be advertised by the MCP server; got: {tools:?}"
            );
        }
    }

    /// Replacement removes only the replaced tools — every other exec tool the subagent did not
    /// claim stays advertised.
    #[test]
    #[serial]
    fn mcp_server_keeps_advertising_exec_tools_a_subagent_did_not_replace() {
        // Given / When
        let tools = with_subagent_replacing(&["Grep", "Glob", "SemanticSearch"], || {
            PermissionServer::new().tool_names()
        });

        // Then
        for kept in ["Read", "Write", "Shell"] {
            assert!(
                tools.contains(&kept.to_string()),
                "non-replaced tool {kept} must still be advertised; got: {tools:?}"
            );
        }
    }

    /// With a subagent that replaces nothing, the full exec catalog stays advertised — enforcement
    /// must not gratuitously drop tools.
    #[test]
    #[serial]
    fn mcp_server_advertises_the_full_exec_catalog_when_nothing_is_replaced() {
        // Given / When
        let tools = with_subagent_replacing(&[], || PermissionServer::new().tool_names());

        // Then
        assert!(
            tools.contains(&"Grep".to_string()),
            "Grep must be advertised when nothing is replaced; got: {tools:?}"
        );
    }
}
