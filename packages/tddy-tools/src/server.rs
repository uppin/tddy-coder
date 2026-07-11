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
use tddy_workflow_recipes::orchestrate_pr_stack::{
    pr_close_action, pr_merge_action, pr_resolve_conflicts_action, GithubPrApi,
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

/// Parameters for a PR-stack tool that acts on one node's open PR by number.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrNodeRefInput {
    #[schemars(description = "Stack node id (e.g. \"n1\").")]
    pub node_id: String,
    #[schemars(description = "The node's open pull request number.")]
    pub pull_number: u64,
}

/// Parameters for [`pr_repoint`](PermissionServer::pr_repoint).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrRepointInput {
    #[schemars(description = "The pull request number to repoint.")]
    pub pull_number: u64,
    #[schemars(description = "New base branch name (e.g. master, or the next unmerged ancestor).")]
    pub new_base: String,
}

/// Parameters for [`pr_resolve_conflicts`](PermissionServer::pr_resolve_conflicts).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrResolveConflictsInput {
    #[schemars(description = "Stack node id whose branch is being synced.")]
    pub node_id: String,
    #[schemars(description = "Absolute path to the node's git worktree.")]
    pub worktree_dir: String,
    #[schemars(description = "Base ref to merge in (e.g. origin/master or an ancestor branch).")]
    pub base_ref: String,
}

/// Parameters for [`pr_set_status`](PermissionServer::pr_set_status).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrSetStatusInput {
    #[schemars(description = "Stack node id to annotate.")]
    pub node_id: String,
    #[schemars(
        description = "Internal status kind (e.g. blocked, needs-repoint, has-conflicts, ready-to-merge, up-to-date, merged)."
    )]
    pub kind: String,
    #[schemars(description = "Optional free-text note explaining the status.")]
    pub note: Option<String>,
}

/// Parameters for [`pr_add_planned`](PermissionServer::pr_add_planned).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrAddPlannedInput {
    #[schemars(description = "PR title.")]
    pub title: String,
    #[schemars(description = "PR description / body.")]
    pub description: String,
    #[schemars(description = "Optional suggested branch name (feature/<stack>/<node>).")]
    pub branch_suggestion: Option<String>,
    #[schemars(description = "Parent node ids (chosen ancestors); empty for a root node.")]
    #[serde(default)]
    pub parents: Vec<String>,
}

/// Parameters for [`pr_spawn_child`](PermissionServer::pr_spawn_child).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrSpawnChildInput {
    #[schemars(description = "Stack node id to start a child coding session for.")]
    pub node_id: String,
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

    #[tool(
        description = "List every PR node in the orchestrator's stack with its live GitHub state and computed internal status (needs-repoint / has-conflicts / ready-to-merge / merged / up-to-date). Refreshes and persists derived statuses; agent overrides are preserved."
    )]
    fn pr_stack_status(&self) -> String {
        match pr_stack_status_impl() {
            Ok(v) => v.to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(description = "Merge a stack node's PR into its base and mark the node merged.")]
    fn pr_merge(&self, Parameters(p): Parameters<PrNodeRefInput>) -> String {
        match (orchestrator_dir(), real_gh()) {
            (Ok(dir), Ok(gh)) => match pr_merge_action(&dir, &gh, &p.node_id, p.pull_number) {
                Ok(sha) => serde_json::json!({ "merged": true, "sha": sha }).to_string(),
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            },
            (Err(e), _) | (_, Err(e)) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(description = "Close a stack node's PR without merging and mark the node closed.")]
    fn pr_close(&self, Parameters(p): Parameters<PrNodeRefInput>) -> String {
        match (orchestrator_dir(), real_gh()) {
            (Ok(dir), Ok(gh)) => match pr_close_action(&dir, &gh, &p.node_id, p.pull_number) {
                Ok(()) => serde_json::json!({ "closed": true }).to_string(),
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            },
            (Err(e), _) | (_, Err(e)) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(
        description = "Repoint a PR's base branch (e.g. after an ancestor merges) via the GitHub REST API."
    )]
    fn pr_repoint(&self, Parameters(p): Parameters<PrRepointInput>) -> String {
        match real_gh() {
            Ok(gh) => match gh.patch_pr_base(p.pull_number, &p.new_base) {
                Ok(()) => {
                    serde_json::json!({ "repointed": true, "new_base": p.new_base }).to_string()
                }
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            },
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(
        description = "Detect conflicts between a node's worktree branch and its base and report the conflicting files. Detect-only: resolve the reported files in the worktree yourself (edit, git add, commit the merge), then re-run to confirm none remain. Marks the node has-conflicts while conflicts exist and clears that marker once the branch merges cleanly."
    )]
    fn pr_resolve_conflicts(&self, Parameters(p): Parameters<PrResolveConflictsInput>) -> String {
        match pr_resolve_conflicts_action(std::path::Path::new(&p.worktree_dir), &p.base_ref) {
            Ok(conflicts) => {
                if conflicts.is_empty() {
                    // Clean now — clear the has-conflicts marker so derivation resumes. Leaves any
                    // other status (e.g. an agent `blocked` override) untouched.
                    let _ = clear_has_conflicts_status(&p.node_id);
                } else {
                    // Sticky (`override`) so a later `pr_stack_status` refresh does not clobber the
                    // conflict signal with a view-derived status.
                    let _ = set_internal_status(&p.node_id, "has-conflicts", None, "override");
                }
                serde_json::json!({ "conflicts": conflicts }).to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "Record a manual internal-status override on a node (e.g. blocked) with an optional note. Overrides are not overwritten by automatic derivation."
    )]
    fn pr_set_status(&self, Parameters(p): Parameters<PrSetStatusInput>) -> String {
        match set_internal_status(&p.node_id, &p.kind, p.note.as_deref(), "override") {
            Ok(()) => serde_json::json!({ "ok": true }).to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(
        description = "Add a new planned PR node to the stack, choosing its ancestors from existing node ids. Returns the server-assigned node id."
    )]
    fn pr_add_planned(&self, Parameters(p): Parameters<PrAddPlannedInput>) -> String {
        let dir = match orchestrator_dir() {
            Ok(d) => d,
            Err(e) => return serde_json::json!({ "error": e }).to_string(),
        };
        let input = tddy_workflow_recipes::pr_stack::AddPlannedPrInput {
            title: p.title,
            description: p.description,
            branch_suggestion: p.branch_suggestion,
            parents: p.parents,
            child_recipe: None,
        };
        match tddy_workflow_recipes::pr_stack::add_planned_pr_node(&dir, input) {
            Ok(node) => serde_json::json!({ "node_id": node.node_id }).to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    #[tool(
        description = "Start a child coding session for a planned PR node (with the orchestrator as its stack parent). Returns the new child session id."
    )]
    async fn pr_spawn_child(&self, Parameters(p): Parameters<PrSpawnChildInput>) -> String {
        // Relay to the daemon over the per-session TDDY_SOCKET. The daemon resolves the node against
        // the orchestrator's stack and spawns a child claude-cli session with stack_parent set —
        // this avoids depending on TDDY_REMOTE_* env (absent for a managed orchestrator).
        let Some(socket) = permission_relay_socket_path() else {
            return serde_json::json!({
                "error": "TDDY_SOCKET is not set; pr_spawn_child requires a managed orchestrator session"
            })
            .to_string();
        };
        let request = serde_json::json!({ "type": "spawn-child", "node_id": p.node_id });
        match crate::toolcall_client::dispatch_toolcall(&socket, request).await {
            Ok(resp) => resp.to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// PR-stack tool helpers
// ---------------------------------------------------------------------------

/// The orchestrator session directory (holds `changeset.yaml` with the stack).
fn orchestrator_dir() -> Result<PathBuf, String> {
    std::env::var_os("TDDY_SESSION_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "TDDY_SESSION_DIR not set (no orchestrator session in scope)".to_string())
}

/// `owner/repo` slug parsed from the repo's `origin` remote.
fn repo_slug() -> Result<String, String> {
    let repo = std::env::var_os("TDDY_REPO_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "TDDY_REPO_DIR not set".to_string())?;
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git remote get-url origin failed: {e}"))?;
    let url = String::from_utf8_lossy(&out.stdout);
    tddy_workflow_recipes::orchestrate_pr_stack::github::owner_repo_from_remote_url(url.trim())
        .ok_or_else(|| format!("could not parse owner/repo from remote url: {}", url.trim()))
}

fn real_gh() -> Result<tddy_workflow_recipes::orchestrate_pr_stack::RealGithubPrApi, String> {
    Ok(tddy_workflow_recipes::orchestrate_pr_stack::RealGithubPrApi::new(repo_slug()?))
}

/// Default branch of the repo (`origin/HEAD` target). Returns an error rather than guessing a name:
/// a wrong default silently mis-derives `needs-repoint`, so the caller surfaces the failure instead.
fn default_branch() -> Result<String, String> {
    let repo = std::env::var_os("TDDY_REPO_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "TDDY_REPO_DIR not set; cannot resolve the default branch".to_string())?;
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .current_dir(&repo)
        .output()
        .map_err(|e| format!("git symbolic-ref refs/remotes/origin/HEAD failed to run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "could not resolve origin/HEAD (run `git remote set-head origin -a`): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let name = s.trim().strip_prefix("origin/").unwrap_or(s.trim());
    if name.is_empty() {
        return Err("origin/HEAD resolved to an empty branch name".to_string());
    }
    Ok(name.to_string())
}

/// Write one node's `internal_status`, applying the override-wins rule for derived writes.
fn set_internal_status(
    node_id: &str,
    kind: &str,
    note: Option<&str>,
    source: &str,
) -> Result<(), String> {
    let dir = orchestrator_dir()?;
    let new_status = tddy_core::changeset::PrInternalStatus {
        kind: kind.to_string(),
        note: note.map(|s| s.to_string()),
        source: source.to_string(),
    };
    tddy_core::changeset::update_stack_atomic(&dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.internal_status = Some(
                tddy_workflow_recipes::orchestrate_pr_stack::reconcile_internal_status(
                    node.internal_status.as_ref(),
                    new_status.clone(),
                ),
            );
        }
    })
    .map_err(|e| e.to_string())
}

/// Clear a node's `internal_status` only when it is currently `has-conflicts` (so a resolved
/// conflict stops being sticky and view-derivation resumes). Any other status — including an agent
/// `blocked` override — is left untouched.
fn clear_has_conflicts_status(node_id: &str) -> Result<(), String> {
    let dir = orchestrator_dir()?;
    tddy_core::changeset::update_stack_atomic(&dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            if node
                .internal_status
                .as_ref()
                .is_some_and(|s| s.kind == "has-conflicts")
            {
                node.internal_status = None;
            }
        }
    })
    .map_err(|e| e.to_string())
}

/// Read the orchestrator stack, refresh derived internal statuses from live GitHub + child state,
/// and return the node summaries as JSON. Live refresh failures are surfaced (never hidden).
fn pr_stack_status_impl() -> Result<serde_json::Value, String> {
    let dir = orchestrator_dir()?;
    let changeset =
        tddy_core::changeset::read_changeset(&dir).map_err(|e| format!("read changeset: {e}"))?;
    let stack = changeset
        .stack
        .clone()
        .ok_or_else(|| "orchestrator changeset has no stack".to_string())?;

    let refresh_error = refresh_internal_statuses(&dir, &stack).err();

    // Re-read after the possible persist so the response reflects what is on disk.
    let stack = tddy_core::changeset::read_changeset(&dir)
        .map_err(|e| format!("re-read changeset: {e}"))?
        .stack
        .unwrap_or(stack);

    let nodes: Vec<serde_json::Value> = stack
        .nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "node_id": n.node_id,
                "title": n.title,
                "branch": n.branch,
                "session_id": n.session_id,
                "pr_status": n.pr_status.as_ref().map(|s| s.phase.clone()),
                "internal_status": n.internal_status.as_ref().map(|s| serde_json::json!({
                    "kind": s.kind,
                    "note": s.note,
                    "source": s.source,
                })),
            })
        })
        .collect();

    let mut out = serde_json::json!({ "nodes": nodes });
    if let Some(err) = refresh_error {
        out["refresh_error"] = serde_json::Value::String(err);
    }
    Ok(out)
}

/// Assemble live views, derive internal statuses, and persist them (override-wins).
fn refresh_internal_statuses(
    dir: &std::path::Path,
    stack: &tddy_core::changeset::Stack,
) -> Result<(), String> {
    let sessions_root = dir
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "cannot derive sessions root from session dir".to_string())?;
    let gh = real_gh()?;
    let default = default_branch()?;
    let views = tddy_workflow_recipes::orchestrate_pr_stack::assemble_views(
        dir,
        sessions_root,
        stack,
        &gh,
        &default,
    )
    .map_err(|e| format!("assemble views: {e}"))?;
    let derived =
        tddy_workflow_recipes::orchestrate_pr_stack::derive_internal_status(&views, &default);
    tddy_core::changeset::update_stack_atomic(dir, |s| {
        for (node_id, d) in &derived {
            if let Some(node) = s.nodes.iter_mut().find(|n| &n.node_id == node_id) {
                node.internal_status = Some(
                    tddy_workflow_recipes::orchestrate_pr_stack::reconcile_internal_status(
                        node.internal_status.as_ref(),
                        d.clone(),
                    ),
                );
            }
        }
    })
    .map_err(|e| format!("persist derived statuses: {e}"))
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

/// One open subagent conversation plus the accounting metadata that lives alongside the session
/// (its agent name and turn count). Cumulative token usage and the model are read back from the
/// session itself (`SubagentSession::cumulative_usage`/`model`).
struct SubagentConversation {
    agent: String,
    turns: u32,
    session: Box<dyn SubagentSession>,
}

type SubagentSessionTable = tokio::sync::Mutex<HashMap<String, SubagentConversation>>;

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
        "usage": {
            "inputTokens": outcome.usage.input_tokens,
            "outputTokens": outcome.usage.output_tokens,
            "totalTokens": outcome.usage.total(),
        },
    })
    .to_string()
}

/// Snapshot every open conversation as the shared [`ConversationRecord`] shape used by
/// `subagent_list` and the accounting file.
fn conversation_records(
    sessions: &HashMap<String, SubagentConversation>,
) -> Vec<tddy_core::token_accounting::ConversationRecord> {
    sessions
        .iter()
        .map(|(id, conv)| {
            let usage = conv.session.cumulative_usage();
            tddy_core::token_accounting::ConversationRecord {
                agent: conv.agent.clone(),
                id: id.clone(),
                model: conv.session.model().to_string(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total(),
                turns: conv.turns,
            }
        })
        .collect()
}

/// Overwrite the host-visible accounting file (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner
/// into the session egress dir) with the current conversation list. A no-op when the env var is
/// unset; write failures are ignored — accounting is best-effort telemetry, never load-bearing.
fn write_accounting_file(sessions: &HashMap<String, SubagentConversation>) {
    let Some(path) = env_non_empty("TDDY_TOOLS_ACCOUNTING_FILE") else {
        return;
    };
    let payload = serde_json::json!({ "conversations": conversation_records(sessions) });
    if let Ok(text) = serde_json::to_string_pretty(&payload) {
        let _ = std::fs::write(&path, text);
    }
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
            subagent_sessions().lock().await.insert(
                session_id.clone(),
                SubagentConversation {
                    agent: agent_name,
                    turns: 0,
                    session,
                },
            );
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
    let Some(conv) = sessions.get_mut(session_id) else {
        return subagent_error_json(format!("unknown subagent session: {session_id}"));
    };
    let response = match conv.session.prompt(&prompt_text).await {
        Ok(outcome) => {
            conv.turns += 1;
            prompt_outcome_json(outcome)
        }
        Err(e) => return subagent_error_json(e),
    };
    write_accounting_file(&sessions);
    response
}

/// `subagent_cancel` (ACP `session/cancel`-shaped): closes an open session, if any.
async fn subagent_cancel_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let mut sessions = subagent_sessions().lock().await;
    let cancelled = sessions.remove(session_id).is_some();
    write_accounting_file(&sessions);
    serde_json::json!({ "cancelled": cancelled }).to_string()
}

/// `subagent_list`: enumerate every open conversation with its per-conversation token accounting.
async fn subagent_list_tool(_args: serde_json::Value) -> String {
    let sessions = subagent_sessions().lock().await;
    serde_json::json!({ "conversations": conversation_records(&sessions) }).to_string()
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

    let list_tool = rmcp::model::Tool::new(
        "subagent_list",
        "List all open subagent conversations with per-conversation token accounting. \
         Returns {conversations:[{agent, id, model, inputTokens, outputTokens, totalTokens, \
         turns}]}.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    );
    router.add_route(subagent_route(list_tool, |args| {
        Box::pin(subagent_list_tool(args))
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
