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
    github::{PrSearchHit, PrState},
    pr_close_action, pr_insight, pr_merge_action, pr_resolve_conflicts_action, GithubPrApi,
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

/// Parameters for [`pr_update_planned`](PermissionServer::pr_update_planned).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrUpdatePlannedInput {
    #[schemars(description = "Stack node id to edit.")]
    pub node_id: String,
    #[schemars(description = "New PR title; omit to leave it unchanged.")]
    pub title: Option<String>,
    #[schemars(description = "New PR description / body; omit to leave it unchanged.")]
    pub description: Option<String>,
    #[schemars(
        description = "New suggested branch name; editable only while the node owns no branch."
    )]
    pub branch_suggestion: Option<String>,
    #[schemars(
        description = "Also push the new title/description to the node's pull request. Rejected when the node has no PR."
    )]
    #[serde(default)]
    pub sync_pr: bool,
}

/// Parameters for [`pr_delete_planned`](PermissionServer::pr_delete_planned).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrDeletePlannedInput {
    #[schemars(description = "Stack node id to remove from the plan.")]
    pub node_id: String,
}

/// Parameters for [`pr_set_parents`](PermissionServer::pr_set_parents).
///
/// `parents` carries **no** `#[serde(default)]`, unlike every other list on this surface: an empty
/// list is a meaningful instruction here (make this node a root), and on a branch-owning node it
/// rebases the branch, force-pushes it with lease and re-targets the pull request. A field the caller
/// forgot must not be read as that instruction, so an omitted `parents` is a parse error and the
/// caller has to write `[]` to mean it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrSetParentsInput {
    #[schemars(description = "Stack node id to move.")]
    pub node_id: String,
    #[schemars(
        description = "The node's whole new parent list (chosen ancestors); pass [] to make it a root off the stack bottom. Required — there is no default."
    )]
    pub parents: Vec<String>,
}

/// Parameters for [`pr_read`](PermissionServer::pr_read).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrReadInput {
    #[schemars(
        description = "Stack node id whose PR to read; name this or pull_number, not both."
    )]
    pub node_id: Option<String>,
    #[schemars(description = "Pull request number to read; name this or node_id, not both.")]
    pub pull_number: Option<u64>,
    #[schemars(description = "Include the changed-file list (omitted by default).")]
    #[serde(default)]
    pub include_files: bool,
}

/// Parameters for [`pr_search`](PermissionServer::pr_search).
///
/// Carries no repository: a search is always scoped to the orchestrator's own remote, resolved by
/// the tool rather than chosen by the agent.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrSearchInput {
    #[schemars(description = "Free text matched against PR titles and bodies.")]
    pub query: Option<String>,
    #[schemars(description = "open / closed / merged / all (default open).")]
    pub state: Option<String>,
    #[schemars(description = "Restrict to PRs opened by this GitHub login.")]
    pub author: Option<String>,
    #[schemars(description = "Restrict to PRs whose base branch is this one.")]
    pub base: Option<String>,
    #[schemars(description = "Maximum hits to return (default 20, hard cap 100).")]
    #[serde(default)]
    pub limit: u32,
}

/// Parameters for [`pr_comments`](PermissionServer::pr_comments).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrCommentsInput {
    #[schemars(
        description = "Stack node id whose PR feedback to read; name this or pull_number, not both."
    )]
    pub node_id: Option<String>,
    #[schemars(description = "Pull request number to read; name this or node_id, not both.")]
    pub pull_number: Option<u64>,
}

/// Parameters for [`pr_adopt`](PermissionServer::pr_adopt).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PrAdoptInput {
    #[schemars(description = "Number of the existing pull request to bring into the stack.")]
    pub pull_number: u64,
    #[schemars(description = "Parent node ids the adopted node stacks on; empty for a root node.")]
    #[serde(default)]
    pub parents: Vec<String>,
}

/// Parameters for [`spawn_conversation`](PermissionServer::spawn_conversation).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpawnConversationInput {
    #[schemars(description = "Prompt to seed the new interactive conversation with.")]
    pub prompt: String,
    #[schemars(
        description = "Optional branch name for the new worktree (derived from the prompt when omitted)."
    )]
    #[serde(default)]
    pub branch: Option<String>,
    #[schemars(
        description = "Optional base ref to root the new worktree on (defaults to the session's base)."
    )]
    #[serde(default)]
    pub base_ref: Option<String>,
}

/// Build the `spawn-conversation` relay request. Pure so it can be unit-tested without a socket;
/// `None` options serialize to JSON `null`.
fn spawn_conversation_request_json(
    prompt: &str,
    branch: Option<&str>,
    base_ref: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "type": "spawn-conversation",
        "prompt": prompt,
        "branch": branch,
        "base_ref": base_ref,
    })
}

/// How [`PermissionServer::call_tool_by_name`] rejects a name it holds no dispatch arm for.
///
/// Public because "is this tool reachable by name" is a contract other crates' tests own — the
/// PR-stack agreement test in `tests/pr_stack_tool_dispatch_acceptance.rs` tells an unregistered name
/// apart from a registered tool's own refusal by exactly this rejection. Named rather than matched as
/// a free-text substring so rewording the message cannot silently turn that test into one that always
/// passes.
pub const UNKNOWN_TOOL_REJECTION: &str = "unknown MCP tool";

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
        let seed_defs = seed_subagents_or_report();
        // Session-tool transport (sandbox IPC or daemon HTTP) present => forward the
        // dynamic exec-tool catalog too, so Claude Code sees Read/Write/Shell/etc.
        // alongside the 3 static tools. Both transport variants use the same static
        // catalog today (see exec_tool_catalog doc comment for why).
        if crate::session_tool_client::detect_session_tool_transport().is_some() {
            // Server-side enforcement of subagent tool replacement: a tool a configured subagent
            // declares it `replaces` is delegated to that subagent, so this server must not
            // advertise it — a direct call must be impossible at the tool server too, not only
            // gated by Claude's allow/disallow lists. Empty when no subagent replaces anything.
            let replaced = resolve_replaced_tools_for_defs(&seed_defs);
            let catalog: Vec<RemoteToolDef> = exec_tool_catalog()
                .into_iter()
                .filter(|tool| !replaced.contains(&tool.name))
                .collect();
            tool_router.merge(dynamic_tool_router(&catalog));
            // Session-action tools (request_action/list_actions/invoke_action). All three are host
            // round-trips over this very transport — `EstablishAction`, `ListActions`,
            // `InvokeAction` — since the session directory the actions live in exists only on the
            // host. So having that surface to reach is the whole of what they need, and the whole
            // of what gates them. Nothing here reads a def's `replaces`: an action surface is not
            // granted by an agent happening to name a particular tool
            // (docs/ft/daemon/session-agent-roster.md § Tool replacement, without behaviour).
            tool_router.merge(crate::action_tools::action_tool_router());
        }
        // Discovery-subagent tools (ACP-shaped: subagent_new_session/prompt/cancel) — registered
        // unconditionally, and *advertised* only while the roster has someone to address (see
        // `advertised_tools`). Registration cannot be the gate any more: an agent attached at
        // minute forty has to become callable without the process restarting, and the router is
        // built once at construction.
        tool_router.merge(subagent_tool_router());
        // LSP tools: the single language-agnostic `Lsp*` set is exposed only when the owner
        // signalled (via `TDDY_LSP_TOOLS`) that a language server is available for the repo.
        // They forward over the same session-tool transport as the exec tools.
        if crate::lsp_tools::lsp_tools_enabled() {
            tool_router.merge(dynamic_tool_router(&crate::lsp_tools::lsp_tool_catalog()));
        }
        Self {
            tool_router,
            socket_path,
        }
    }

    /// Every tool name this server advertises right now — the exact set `tools/list` will report,
    /// including any merged-in dynamic exec tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.advertised_tools()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// The tools `tools/list` reports: everything in this server's router, minus the subagent
    /// conversation tools while **no agent is attached**, and minus every exec tool an attached agent
    /// has taken over.
    ///
    /// Computed per call rather than baked into the router at construction, because the roster is
    /// live: an attach makes the conversation tools appear and takes the replaced exec tools away, a
    /// detach does the reverse, and each applied revision sends `notifications/tools/list_changed` so
    /// the main agent re-lists and sees it (docs/ft/daemon/session-agent-roster.md § The roster
    /// stream). Advertising the conversation tools on an empty roster would show the operator four
    /// tools that can only answer "no agents are attached"; advertising a replaced exec tool would
    /// keep the main agent reaching for a tool the call-time check refuses mid-turn instead of
    /// delegating to the agent that now serves it.
    ///
    /// Both halves of the answer come from one read of the roster
    /// ([`LiveAgentRoster::catalog_visibility`](crate::session_agents::LiveAgentRoster::catalog_visibility)),
    /// so the advertised set always describes a single revision.
    ///
    /// The withheld set is the roster's, not the spawn seed's, so it cannot disagree with the refusal
    /// a direct call meets — both are
    /// [`LiveAgentRoster::withdrawn_exec_tools`](crate::session_agents::LiveAgentRoster::withdrawn_exec_tools).
    fn advertised_tools(&self) -> Vec<rmcp::model::Tool> {
        let visible = crate::session_agents::session_agent_roster().catalog_visibility();
        let mut tools = self.tool_router.list_all();
        if !visible.has_an_agent_to_address {
            let conversation_tools = subagent_tool_names();
            tools.retain(|tool| !conversation_tools.contains(&tool.name.to_string()));
        }
        tools.retain(|tool| {
            visible
                .withdrawn_exec_tools
                .taken_over_by(tool.name.as_ref())
                .is_none()
        });
        tools
    }

    /// Enumerate every tool this server would advertise to an agent, as [`RemoteToolDef`]s
    /// (name + description + JSON input schema): the static workflow `#[tool]`s, the exec-tool
    /// catalog (unconditionally — Read/Write/Shell/…), and the subagent tools while an agent is
    /// attached. Pure enumeration (no socket/session) for `tddy-tools list-tools`, which feeds the
    /// web Inspector → Tools panel. Does NOT include the Bash CLI subcommands (submit/ask/…); the
    /// `list-tools` command appends those.
    ///
    /// Not the same set as [`Self::advertised_tools`], despite the name: this one answers "what
    /// tools does this build have" and so lists the exec catalog whole, withdrawn entries included,
    /// while that one answers "what may this session's agent call right now" and filters them out.
    /// The single-revision guarantee documented there is that method's, not this file's — an
    /// operator reading the Inspector wants the build's inventory, and an agent must never be
    /// offered a tool an operator withdrew.
    pub fn advertised_tool_defs() -> Vec<RemoteToolDef> {
        fn map_tool(t: rmcp::model::Tool) -> RemoteToolDef {
            RemoteToolDef {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema_json: serde_json::to_string(&*t.input_schema)
                    .unwrap_or_else(|_| "{}".to_string()),
            }
        }
        let mut defs: Vec<RemoteToolDef> = Self::tool_router()
            .list_all()
            .into_iter()
            .map(map_tool)
            .collect();
        defs.extend(exec_tool_catalog());
        if !crate::session_agents::session_agent_roster().is_empty() {
            defs.extend(subagent_tool_router().list_all().into_iter().map(map_tool));
        }
        defs
    }

    /// Invoke one of the workflow `#[tool]` methods by name with JSON `args`, returning its result
    /// string. Used by `tddy-tools call-tool` (the web Inspector → Tools "invoke" button) to run a
    /// tool exactly as the agent would over MCP. We dispatch to the methods directly rather than via
    /// the rmcp `ToolRouter`, because a router call needs a live `Peer`/`RequestContext` that can't
    /// be fabricated outside a real MCP connection. Relay tools (`spawn_conversation`,
    /// `pr_spawn_child`) still relay over `TDDY_SOCKET` from inside their methods; the rest run
    /// in-process against `TDDY_SESSION_DIR`/`TDDY_REPO_DIR`.
    pub async fn call_tool_by_name(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        fn parse<T: serde::de::DeserializeOwned>(args: serde_json::Value) -> Result<T, String> {
            let args = if args.is_null() {
                serde_json::Value::Object(Default::default())
            } else {
                args
            };
            serde_json::from_value(args).map_err(|e| {
                format!(
                    "invalid arguments for `{}`: {e}",
                    std::any::type_name::<T>()
                )
            })
        }
        Ok(match name {
            "approval_prompt" => self.approval_prompt(Parameters(parse(args)?)),
            "github_create_pull_request" => {
                self.github_create_pull_request(Parameters(parse(args)?))
            }
            "github_update_pull_request" => {
                self.github_update_pull_request(Parameters(parse(args)?))
            }
            "pr_stack_status" => self.pr_stack_status(),
            "pr_merge" => self.pr_merge(Parameters(parse(args)?)),
            "pr_close" => self.pr_close(Parameters(parse(args)?)),
            "pr_repoint" => self.pr_repoint(Parameters(parse(args)?)),
            "pr_resolve_conflicts" => self.pr_resolve_conflicts(Parameters(parse(args)?)),
            "pr_set_status" => self.pr_set_status(Parameters(parse(args)?)),
            "pr_add_planned" => self.pr_add_planned(Parameters(parse(args)?)),
            "pr_update_planned" => self.pr_update_planned(Parameters(parse(args)?)),
            "pr_delete_planned" => self.pr_delete_planned(Parameters(parse(args)?)),
            "pr_set_parents" => self.pr_set_parents(Parameters(parse(args)?)),
            "pr_read" => self.pr_read(Parameters(parse(args)?)),
            "pr_search" => self.pr_search(Parameters(parse(args)?)),
            "pr_comments" => self.pr_comments(Parameters(parse(args)?)),
            "pr_adopt" => self.pr_adopt(Parameters(parse(args)?)),
            "pr_spawn_child" => self.pr_spawn_child(Parameters(parse(args)?)).await,
            "spawn_conversation" => self.spawn_conversation(Parameters(parse(args)?)).await,
            other => return Err(format!("{UNKNOWN_TOOL_REJECTION}: {other}")),
        })
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
        to_wire(pr_stack_status_impl())
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

    #[tool(
        description = "Edit a stack node's title, description and/or branch_suggestion. Title and description are editable at any time, including once the node owns a branch, a child session and an open PR; branch_suggestion only while the node owns no branch. A call naming none of the three is rejected. With sync_pr, the same title/description are also pushed to the node's pull request — rejected, before anything is written, when the node records no PR or when neither a title nor a description was given."
    )]
    fn pr_update_planned(&self, Parameters(p): Parameters<PrUpdatePlannedInput>) -> String {
        to_wire(pr_update_planned_impl(p))
    }

    #[tool(
        description = "Remove a node from the plan, reparenting its children onto that node's parents so the DAG stays connected (a deleted root's children become roots). Refuses a node whose PR is open — merge or close it first. The node's branch, worktree and child session are left untouched and reported back as now unowned."
    )]
    fn pr_delete_planned(&self, Parameters(p): Parameters<PrDeletePlannedInput>) -> String {
        to_wire(pr_delete_planned_impl(&p.node_id))
    }

    #[tool(
        description = "Move a node in the stack by giving it a whole new parent list. parents is required: pass [] to make the node a root off the stack bottom. Rejects an unknown parent, self-parenthood, a duplicate entry, and any change that would close a cycle, writing nothing. When the node owns a branch, its branch is also rebased onto the new effective base, force-pushed with lease, and its open PR's base repointed. Use this when the plan changed; use pr_repoint when only the PR's base branch drifted."
    )]
    fn pr_set_parents(&self, Parameters(p): Parameters<PrSetParentsInput>) -> String {
        to_wire(pr_set_parents_impl(p))
    }

    #[tool(
        description = "Read one pull request in full: title, body, state, base/head, mergeability, size, the latest review state per reviewer, and the head commit's check runs. Address it by exactly one of node_id or pull_number — naming neither or both is rejected, and a node that records no PR url cannot be addressed by node_id. Pass include_files for the changed-file list, which is otherwise neither fetched nor returned."
    )]
    fn pr_read(&self, Parameters(p): Parameters<PrReadInput>) -> String {
        to_wire(pr_read_impl(p))
    }

    #[tool(
        description = "Search this repository's pull requests, including ones the stack does not track, by text, state (open/closed/merged/all, default open), author and base branch. The repository is always the orchestrator's own and cannot be chosen. Returns at most limit hits (default 20, hard cap 100), each with number, title, state, draft, author, url and updated_at — GitHub's search reports no head or base branch, so follow up with pr_read when you need them."
    )]
    fn pr_search(&self, Parameters(p): Parameters<PrSearchInput>) -> String {
        to_wire(pr_search_impl(p))
    }

    #[tool(
        description = "Read a pull request's review feedback as three separate sections: submitted reviews, diff-anchored comment threads in reply order, and conversation comments. Address it by exactly one of node_id or pull_number — naming neither or both is rejected. A thread's resolved/unresolved state is not available over this API, so no thread reports one: read the replies to judge."
    )]
    fn pr_comments(&self, Parameters(p): Parameters<PrCommentsInput>) -> String {
        to_wire(pr_comments_impl(p))
    }

    #[tool(
        description = "Bring an existing pull request into the stack as a node bound to its head branch and PR reference, carrying the PR's title, body and live phase, choosing which existing nodes it stacks on (empty for a root). Refuses a PR whose head branch is already bound to a node, and an unknown or cycle-forming parent. The adopted node has no child session."
    )]
    fn pr_adopt(&self, Parameters(p): Parameters<PrAdoptInput>) -> String {
        to_wire(pr_adopt_impl(p))
    }

    #[tool(
        description = "Start a brand-new interactive coding conversation on a fresh worktree, seeded with the given prompt and tagged with the current session as its orchestrator. Returns the new child session id."
    )]
    async fn spawn_conversation(
        &self,
        Parameters(p): Parameters<SpawnConversationInput>,
    ) -> String {
        // Relay to the daemon over the per-session TDDY_SOCKET. The daemon spawns a new claude-cli
        // conversation on a new worktree tagged with this session as its orchestrator — the generic
        // sibling of `pr_spawn_child`, available to any managed session (e.g. grill-me).
        let Some(socket) = permission_relay_socket_path() else {
            return serde_json::json!({
                "error": "TDDY_SOCKET is not set; spawn_conversation requires a managed session"
            })
            .to_string();
        };
        let request =
            spawn_conversation_request_json(&p.prompt, p.branch.as_deref(), p.base_ref.as_deref());
        match crate::toolcall_client::dispatch_toolcall(&socket, request).await {
            Ok(resp) => resp.to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// PR-stack tool helpers
// ---------------------------------------------------------------------------

/// The state a `pr_search` that expressed no preference is run with: the PRs still in play.
const DEFAULT_SEARCH_STATE: &str = "open";

/// Serialize a PR-stack tool's outcome for the wire: the value itself, or `{"error": …}`.
///
/// Every `#[tool]` here returns a `String`, so a failure has to reach the agent as JSON rather than
/// as a transport error — this is the one envelope all of them share.
fn to_wire(result: Result<serde_json::Value, String>) -> String {
    match result {
        Ok(v) => v.to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

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

/// Edit a node's metadata and, when asked, push the same title/body to its pull request.
///
/// Everything a `sync_pr` push can refuse without touching the network is checked *before* the local
/// edit, so a refused call leaves the plan exactly as it was — the same contract every other
/// operation on this surface keeps. Only a failure GitHub itself returns can leave the node edited
/// and its pull request not, and that is reported as such.
fn pr_update_planned_impl(p: PrUpdatePlannedInput) -> Result<serde_json::Value, String> {
    let dir = orchestrator_dir()?;

    // Resolved *before* the local edit. Each of these can refuse without touching the network, and a
    // refusal that has already rewritten the plan is a partial application of a call the caller was
    // told had failed: the payload the push would carry, the PR the node records, and the credential.
    let sync_client = if p.sync_pr {
        if p.title.is_none() && p.description.is_none() {
            return Err(format!(
                "pr_update_planned: node '{}' was asked to sync its pull request, but neither a \
                 title nor a description was given — GitHub refuses an empty edit, so there is \
                 nothing to push and nothing was changed",
                p.node_id
            ));
        }
        addressed_pull_number(Some(&p.node_id), None)?;
        Some(real_gh()?)
    } else {
        None
    };

    let node = tddy_workflow_recipes::pr_stack::update_planned_pr_node(
        &dir,
        tddy_workflow_recipes::pr_stack::UpdatePlannedPrInput {
            node_id: p.node_id.clone(),
            title: p.title.clone(),
            description: p.description.clone(),
            branch_suggestion: p.branch_suggestion,
        },
    )?;
    let mut out = serde_json::json!({
        "node_id": node.node_id,
        "title": node.title,
        "description": node.description,
        "branch_suggestion": node.branch_suggestion,
        "branch": node.branch,
        "session_id": node.session_id,
        "parents": node.parents,
    });
    if let Some(gh) = sync_client {
        // The preconditions held, so a failure here is GitHub itself refusing the write — the plan is
        // already edited and the pull request is not, which the caller has to be told.
        let number = tddy_workflow_recipes::pr_stack::sync_node_to_github_pr(
            &dir,
            &p.node_id,
            p.title.as_deref(),
            p.description.as_deref(),
            &gh,
        )
        .map_err(|e| format!("{e} (the node was updated; its pull request was not)"))?;
        out["pr_synced"] = serde_json::json!(number);
    }
    Ok(out)
}

/// Remove a node from the plan and report what the removal left behind.
fn pr_delete_planned_impl(node_id: &str) -> Result<serde_json::Value, String> {
    let dir = orchestrator_dir()?;
    let deleted = tddy_workflow_recipes::pr_stack::delete_planned_pr_node(&dir, node_id)?;
    Ok(serde_json::json!({
        "deleted": deleted.node.node_id,
        "reparented_children": deleted.reparented_children,
        "orphaned_branch": deleted.orphaned_branch,
        "orphaned_session_id": deleted.orphaned_session_id,
    }))
}

/// Rewrite a node's parents, realigning git and its PR base when the node owns a branch.
fn pr_set_parents_impl(p: PrSetParentsInput) -> Result<serde_json::Value, String> {
    let dir = orchestrator_dir()?;
    let repo_root = std::env::var_os("TDDY_REPO_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "TDDY_REPO_DIR not set; cannot realign a moved node's branch".to_string())?;
    let gh = real_gh()?;
    let default = default_branch()?;
    let node = tddy_workflow_recipes::pr_stack::set_stack_node_parents(
        &dir, &repo_root, &p.node_id, &p.parents, &default, &gh,
    )?;
    Ok(serde_json::json!({ "node_id": node.node_id, "parents": node.parents }))
}

/// Read one pull request in full.
fn pr_read_impl(p: PrReadInput) -> Result<serde_json::Value, String> {
    let number = addressed_pull_number(p.node_id.as_deref(), p.pull_number)?;
    let gh = real_gh()?;
    let view =
        pr_insight::read_pr(&gh, number, p.include_files).map_err(|e| format!("read pr: {e}"))?;
    Ok(pr_read_json(&view))
}

/// Read a pull request's review feedback, split into reviews, threads and conversation.
fn pr_comments_impl(p: PrCommentsInput) -> Result<serde_json::Value, String> {
    let number = addressed_pull_number(p.node_id.as_deref(), p.pull_number)?;
    let gh = real_gh()?;
    let view =
        pr_insight::read_pr_comments(&gh, number).map_err(|e| format!("read pr comments: {e}"))?;
    Ok(pr_comments_json(&view))
}

/// Shape a [`pr_insight::PrCommentsView`] for the wire.
///
/// Owned by the tool for the same reason as [`pr_read_json`]: the agent-facing shape is a deliberate
/// contract rather than whatever field list the Rust types happen to carry. The three sections stay
/// separate keys because a verdict, a diff-anchored conversation and a PR-wide comment answer
/// different questions.
fn pr_comments_json(view: &pr_insight::PrCommentsView) -> serde_json::Value {
    serde_json::json!({
        "reviews": view.reviews.iter().map(|r| serde_json::json!({
            "author": r.author,
            "state": r.state,
            "body": r.body,
            "submitted_at": r.submitted_at,
        })).collect::<Vec<_>>(),
        // No `resolved` on a thread: resolution state exists only on GitHub's GraphQL
        // `reviewThreads`, and emitting a guessed value would mislead the agent.
        "threads": view.threads.iter().map(|t| serde_json::json!({
            "path": t.path,
            "line": t.line,
            "diff_hunk": t.diff_hunk,
            "comments": t.comments.iter().map(|c| serde_json::json!({
                "author": c.author,
                "body": c.body,
                "created_at": c.created_at,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "conversation": view.conversation.iter().map(|c| serde_json::json!({
            "author": c.author,
            "body": c.body,
            "created_at": c.created_at,
        })).collect::<Vec<_>>(),
    })
}

/// Search the orchestrator's own repository for pull requests.
///
/// The repository is resolved here and passed in; the agent's input carries no repository at all, so
/// a search can never reach another one.
fn pr_search_impl(p: PrSearchInput) -> Result<serde_json::Value, String> {
    let repo = repo_slug()?;
    // Built from the slug already resolved above rather than through `real_gh()`, which would run
    // `git remote get-url origin` a second time for the same answer.
    let gh = tddy_workflow_recipes::orchestrate_pr_stack::RealGithubPrApi::new(repo.clone());
    let hits = pr_insight::search_repository_prs(
        &gh,
        &repo,
        pr_insight::PrSearchInput {
            text: p.query,
            state: p.state.unwrap_or_else(|| DEFAULT_SEARCH_STATE.to_string()),
            author: p.author,
            base: p.base,
            // `0` is "no preference" to `search_repository_prs`, which owns the default and the cap.
            limit: p.limit,
        },
    )
    .map_err(|e| format!("search prs: {e}"))?;
    Ok(pr_search_json(&hits))
}

/// Shape a page of search hits for the wire.
///
/// Owned by the tool for the same reason as [`pr_read_json`]: the agent-facing shape is a deliberate
/// contract rather than whatever field list the Rust types happen to carry. No branch fields —
/// GitHub's search reports neither head nor base, so a hit that needs them is followed up with
/// `pr_read`.
fn pr_search_json(hits: &[PrSearchHit]) -> serde_json::Value {
    serde_json::json!({
        "hits": hits.iter().map(|h| serde_json::json!({
            "number": h.number,
            "title": h.title,
            "state": h.state,
            "draft": h.draft,
            "author": h.author,
            "url": h.url,
            "updated_at": h.updated_at,
        })).collect::<Vec<_>>(),
    })
}

/// Create a stack node from an existing pull request.
fn pr_adopt_impl(p: PrAdoptInput) -> Result<serde_json::Value, String> {
    let dir = orchestrator_dir()?;
    let gh = real_gh()?;
    let node =
        tddy_workflow_recipes::pr_stack::adopt_pr_into_stack(&dir, p.pull_number, p.parents, &gh)?;
    Ok(serde_json::json!({
        "node_id": node.node_id,
        "branch": node.branch,
        "parents": node.parents,
        "pr_status": node.pr_status.as_ref().map(|s| s.phase.clone()),
    }))
}

/// Resolve which pull request a read tool was asked about.
///
/// Exactly one of `node_id` / `pull_number` addresses a PR: naming neither leaves nothing to read,
/// and naming both lets the two disagree, so both are rejected rather than settled by a precedence
/// rule the agent would have to know about.
fn addressed_pull_number(node_id: Option<&str>, pull_number: Option<u64>) -> Result<u64, String> {
    match (node_id, pull_number) {
        (Some(node_id), None) => {
            let dir = orchestrator_dir()?;
            let stack = tddy_core::changeset::read_changeset(&dir)
                .map_err(|e| format!("read changeset: {e}"))?
                .stack
                .ok_or_else(|| "orchestrator changeset has no stack".to_string())?;
            pr_insight::pull_number_for_node(&stack, node_id)
        }
        (None, Some(number)) => Ok(number),
        (None, None) => {
            Err("name the pull request to read: pass node_id or pull_number".to_string())
        }
        (Some(_), Some(_)) => {
            Err("node_id and pull_number both name a pull request; pass exactly one".to_string())
        }
    }
}

/// Shape a [`pr_insight::PrReadView`] for the wire.
///
/// Owned by the tool rather than by a `Serialize` derive on the view, so the agent-facing shape is a
/// deliberate contract instead of whatever field list the Rust type happens to carry. `files` is
/// absent (not `null`) when the caller did not ask for it, matching "neither fetched nor returned".
fn pr_read_json(view: &pr_insight::PrReadView) -> serde_json::Value {
    let mut out = serde_json::json!({
        "number": view.number,
        "url": view.url,
        "title": view.title,
        "body": view.body,
        "state": pr_state_name(view.state),
        "base": view.base_branch,
        "head": view.head_branch,
        "head_sha": view.head_sha,
        "mergeable": view.mergeable,
        "mergeable_state": view.mergeable_state,
        "additions": view.additions,
        "deletions": view.deletions,
        "changed_files": view.changed_files,
        "reviews": view.reviews.iter().map(|r| serde_json::json!({
            "author": r.author,
            "state": r.state,
        })).collect::<Vec<_>>(),
        "checks": view.checks.iter().map(|c| serde_json::json!({
            "name": c.name,
            "conclusion": c.conclusion,
        })).collect::<Vec<_>>(),
    });
    if let Some(files) = &view.files {
        out["files"] = files
            .iter()
            .map(|f| serde_json::json!({ "path": f.path, "status": f.status }))
            .collect::<Vec<_>>()
            .into();
    }
    out
}

/// A PR's live state in the lowercase vocabulary the tool surface already uses for a node's phase,
/// keeping `draft` distinct from `open` — the two differ to a reviewer.
fn pr_state_name(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
        PrState::Draft => "draft",
    }
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

    /// Answered from [`PermissionServer::advertised_tools`] rather than from the router directly,
    /// so the subagent conversation tools follow the live roster. `#[tool_handler]` only generates
    /// the methods an impl does not already define, so this replaces its `list_tools` and leaves
    /// `call_tool` alone — a call to a tool that is registered but currently unadvertised is
    /// answered by its own handler's refusal, which names what is missing.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.advertised_tools(),
            meta: None,
            next_cursor: None,
        })
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
///
/// A tool the session's **live** roster has withdrawn is refused here rather than run. This is the
/// second of the two enforcement layers in docs/ft/daemon/session-agent-roster.md § Enforced at two
/// layers: `--allowedTools` is fixed when `claude` spawns, so an agent attached at minute forty can
/// only take a tool over by having the call refused where it is made. The refusal is hard — there is
/// no path that runs the tool anyway.
pub async fn dispatch_dynamic_tool(tool_name: &str, args: serde_json::Value) -> String {
    let roster = crate::session_agents::session_agent_roster();
    if let Err(refusal) = roster.check_tool_available(tool_name) {
        log::info!(target: "tddy_tools::server", "refusing {tool_name}: {refusal}");
        return serde_json::json!({"error": refusal.to_string(), "is_error": true}).to_string();
    }
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

/// The names of the ACP-shaped conversation tools, read off the router that defines them so the
/// advertisement filter cannot drift from the set it filters.
fn subagent_tool_names() -> Vec<String> {
    subagent_tool_router()
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

/// Why an attached agent cannot be run from here: its def lives on the daemon that owns it.
///
/// Shared by every tool that opens a turn loop, so the main agent reads one wording for one
/// condition however it arrived at it.
fn unreachable_agent_error(entry: &tddy_service::proto::connection::SessionAgentEntry) -> String {
    format!(
        "agent '{}' is attached but this session cannot reach it: its conversations are routed \
         by daemon '{}', which this build does not yet ask",
        entry.agent_id, entry.daemon_instance_id
    )
}

/// Open a turn loop with the roster agent `agent_id`, for a tool that runs one bounded exchange of
/// its own instead of handing a conversation to the main agent (`request_action`).
///
/// Resolved against the live roster exactly as [`subagent_new_session_tool`] resolves it — same
/// ids, same refusals, no default for a call that names none — so which agents are addressable does
/// not depend on which tool is asking, and no tool confers a role on an agent by inspecting what it
/// `replaces`.
///
/// No conversation is registered with the roster: the exchange opens and ends inside the call, so
/// there is nothing a later `subagent_cancel` or a detach could address.
pub(crate) fn open_roster_agent_session(
    agent_id: &str,
) -> Result<(String, Box<dyn SubagentSession>), String> {
    let roster = crate::session_agents::session_agent_roster();
    let entry = roster.resolve(Some(agent_id)).map_err(|e| e.to_string())?;
    let def = roster
        .local_def_for(&entry)
        .ok_or_else(|| unreachable_agent_error(&entry))?;
    let name = def.name.clone();
    let session = SubagentRegistry::from_defs(vec![def])
        .create(&name, subagent_config_from_env())
        .map_err(|e| format!("agent '{}': {e}", entry.agent_id))?;
    Ok((entry.agent_id, session))
}

pub(crate) fn env_non_empty(key: &str) -> Option<String> {
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

/// Every conversation this process has run: the ones still open, and the accounting of the ones
/// that ended.
#[derive(Default)]
struct SubagentConversations {
    open: HashMap<String, SubagentConversation>,
    /// Conversations that ended — cancelled by the main agent, or whose agent was detached
    /// underneath them.
    ///
    /// Their tokens were spent, so they stay enumerable: the accounting file is rewritten wholesale
    /// from this table, and dropping a conversation before the rewrite erases its totals from the
    /// host's view of what the session cost.
    retired: Vec<tddy_core::token_accounting::ConversationRecord>,
}

impl SubagentConversations {
    /// End `conversation_id`, keeping its accounting. Returns whether it was open.
    fn retire(&mut self, conversation_id: &str) -> bool {
        let Some(conversation) = self.open.remove(conversation_id) else {
            return false;
        };
        self.retired
            .push(conversation_record(conversation_id, &conversation));
        true
    }
}

type SubagentSessionTable = tokio::sync::Mutex<SubagentConversations>;

/// Process-wide session table — `PermissionServer` merges the subagent router at construction
/// time, but the conversation must survive across separate `tools/call` invocations, so the table
/// lives outside any single `PermissionServer` instance.
fn subagent_sessions() -> &'static SubagentSessionTable {
    static SESSIONS: OnceLock<SubagentSessionTable> = OnceLock::new();
    SESSIONS.get_or_init(|| tokio::sync::Mutex::new(SubagentConversations::default()))
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
/// process. Empty when the env var is unset or blank: with no def there is no agent, since every
/// agent this process can address came from a def source someone wrote.
///
/// A value that is *set* and does not parse is an error, never an empty seed. `SpecializedAgentDef`
/// is `deny_unknown_fields`, so a `tddy-tools` older than the daemon that wrote the value parses
/// exactly this way — and an empty seed means no agent is attached and none of the withdrawn tools
/// are served by anyone, with nothing naming the variable that caused it.
///
/// The message carries serde's position, never the value: a def carries a provider credential.
pub fn subagents_from_env() -> Result<Vec<SpecializedAgentDef>, String> {
    let Some(json) = env_non_empty("TDDY_SUBAGENTS_JSON") else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<SpecializedAgentDef>>(&json).map_err(|e| {
        format!(
            "TDDY_SUBAGENTS_JSON is set but does not parse as an array of agent defs: {e}. \
             This is what a tddy-tools older than the daemon that spawned it sees, and treating \
             it as 'no agents are attached' would silently un-withdraw every tool the session's \
             agents took over"
        )
    })
}

/// The spawn seed for the two lazy constructions that have no caller to refuse to — the MCP
/// server's router and the process-wide roster.
///
/// `--mcp` already refused to start on an unparseable value (see `run_mcp_server`), so reaching the
/// error arm means a caller that never passed that gate. It is reported at `error` naming the
/// variable rather than passed off as a session nobody attached an agent to.
pub(crate) fn seed_subagents_or_report() -> Vec<SpecializedAgentDef> {
    subagents_from_env().unwrap_or_else(|e| {
        log::error!(target: "tddy_tools::server", "{e}");
        Vec::new()
    })
}

/// The only thing a caller supplies that a def cannot: how this process reaches the codebase.
/// Endpoint, model, credential and turn budget come from the def itself.
pub(crate) fn subagent_config_from_env() -> SubagentConfig {
    SubagentConfig {
        access: subagent_codebase_access_from_env(),
    }
}

pub(crate) fn subagent_error_json(message: impl std::fmt::Display) -> String {
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

/// One conversation as the shared [`tddy_core::token_accounting::ConversationRecord`] shape used by
/// `subagent_list` and the accounting file.
fn conversation_record(
    id: &str,
    conv: &SubagentConversation,
) -> tddy_core::token_accounting::ConversationRecord {
    let usage = conv.session.cumulative_usage();
    tddy_core::token_accounting::ConversationRecord {
        agent: conv.agent.clone(),
        id: id.to_string(),
        model: conv.session.model().to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total(),
        turns: conv.turns,
    }
}

/// Every conversation this process has run, open ones first. The retired ones are included because
/// their tokens were spent by this session: an accounting file that lists only what is still open
/// reports a detached agent's consumption as zero.
fn conversation_records(
    conversations: &SubagentConversations,
) -> Vec<tddy_core::token_accounting::ConversationRecord> {
    conversations
        .open
        .iter()
        .map(|(id, conv)| conversation_record(id, conv))
        .chain(conversations.retired.iter().cloned())
        .collect()
}

/// Overwrite the host-visible accounting file (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner
/// into the session egress dir) with the current conversation list. A no-op when the env var is
/// unset; write failures are ignored — accounting is best-effort telemetry, never load-bearing.
fn write_accounting_file(conversations: &SubagentConversations) {
    let Some(path) = env_non_empty("TDDY_TOOLS_ACCOUNTING_FILE") else {
        return;
    };
    let payload = serde_json::json!({ "conversations": conversation_records(conversations) });
    if let Ok(text) = serde_json::to_string_pretty(&payload) {
        let _ = std::fs::write(&path, text);
    }
}

/// `subagent_new_session` (ACP `session/new`-shaped): opens a conversation with the named
/// subagent under the given `sessionId` — the caller decides the conversation id; one is generated
/// only when omitted.
///
/// The agent is resolved against the session's **live roster**, not against the spawn env: an agent
/// attached after this process started is callable, and one detached is refused naming the id (see
/// docs/ft/daemon/session-agent-roster.md § Invoking an agent).
///
/// A call naming no agent is an error listing the agents there are. There is no default: with any
/// number of agents attachable, picking one for the caller would make the choice depend on attach
/// order rather than on what the main agent asked for.
async fn subagent_new_session_tool(args: serde_json::Value) -> String {
    let roster = crate::session_agents::session_agent_roster();
    let agent_id = args
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let entry = match roster.open_conversation_as(&session_id, agent_id) {
        Ok(entry) => entry,
        Err(e) => return subagent_error_json(e),
    };
    // A roster entry carries no endpoint, credential or turn budget — deliberately, so editing a def
    // cannot change what a running session may call. This process can therefore only run the loop of
    // an agent it holds the def for.
    //
    // TODO(session-agent-roster): route the rest through the facilitating daemon's
    // `OpenAgentConversation` / `PromptAgentConversation` / `CancelAgentConversation`, which is what
    // makes a remote agent — and a local one attached after spawn — callable at all. Those handlers
    // are the roster's daemon-side tranche.
    let Some(def) = roster.local_def_for(&entry) else {
        roster.close_conversation(&session_id);
        return subagent_error_json(unreachable_agent_error(&entry));
    };
    let agent_name = def.name.clone();
    let registry = SubagentRegistry::from_defs(vec![def]);
    match registry.create(&agent_name, subagent_config_from_env()) {
        Ok(session) => {
            subagent_sessions().lock().await.open.insert(
                session_id.clone(),
                SubagentConversation {
                    agent: entry.agent_id,
                    turns: 0,
                    session,
                },
            );
            serde_json::json!({ "sessionId": session_id }).to_string()
        }
        Err(e) => {
            roster.close_conversation(&session_id);
            subagent_error_json(e)
        }
    }
}

/// `subagent_prompt` (ACP `session/prompt`-shaped): sends one prompt turn to an already-open
/// session and returns `{stopReason, content}` once the subagent yields.
///
/// A conversation whose agent was detached underneath it is refused naming the detach rather than
/// prompted: the main agent waits on this call, so a conversation that can no longer be answered has
/// to fail rather than hang.
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
    if sessions.open.contains_key(session_id) {
        let roster = crate::session_agents::session_agent_roster();
        if let crate::session_agents::ConversationState::Cancelled { reason } =
            roster.conversation_state(session_id)
        {
            // Its agent is no longer attached, so drop the loop still holding the conversation's
            // history rather than leaving a session behind that can never be prompted again. Its
            // accounting is retired rather than dropped: those tokens were spent, and the file
            // below is rewritten wholesale.
            sessions.retire(session_id);
            // The roster tracked this conversation only so it could be cancelled; nothing will ask
            // about it again, and a cancelled entry nobody forgets is rescanned by every later
            // frame for the process lifetime.
            roster.close_conversation(session_id);
            write_accounting_file(&sessions);
            return subagent_error_json(reason);
        }
    }
    let Some(conv) = sessions.open.get_mut(session_id) else {
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
    // Retired rather than forgotten: a cancelled conversation's tokens were spent by this session,
    // and the accounting file below is rewritten from this table wholesale.
    let cancelled = sessions.retire(session_id);
    // The roster tracks the same conversation, so that a later detach of its agent does not report
    // a cancellation for something the main agent already closed.
    crate::session_agents::session_agent_roster().close_conversation(session_id);
    write_accounting_file(&sessions);
    serde_json::json!({ "cancelled": cancelled }).to_string()
}

/// `subagent_list`: enumerate every conversation this session ran — open and ended — with its
/// per-conversation token accounting.
async fn subagent_list_tool(_args: serde_json::Value) -> String {
    let sessions = subagent_sessions().lock().await;
    serde_json::json!({ "conversations": conversation_records(&sessions) }).to_string()
}

pub(crate) fn schema_object(
    json: serde_json::Value,
) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    std::sync::Arc::new(json.as_object().cloned().unwrap_or_default())
}

/// Wraps a subagent tool handler (`async fn(Value) -> String`) into a `ToolRoute` — the same
/// success-envelope-with-embedded-error convention `dynamic_tool_router` uses for exec tools.
pub(crate) fn subagent_route<F>(
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
                "agent": {"type": "string", "description": "Required. The name of the subagent attached to this session to open a conversation with."},
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
    use rstest::rstest;
    use serial_test::serial;
    use tddy_workflow_recipes::orchestrate_pr_stack::github::{
        CheckRun, PrFile, PrIssueComment, PrReview,
    };

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
            r#"[{{"name":"explorer","model":"m","base_url":"http://127.0.0.1:1","replaces":[{}]}}]"#,
            replaced
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        std::env::set_var(IPC_SOCKET_ENV, "/tmp/tddy-test-ipc.sock");
        std::env::set_var("TDDY_SUBAGENT", "explorer");
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

    /// `spawn_conversation` refuses to act when there is no `TDDY_SOCKET` to relay over — a plain
    /// (non-managed) session cannot spawn a follow-up conversation. In test builds the socket is
    /// disabled unless `TDDY_TOOLS_TEST_ALLOW_SOCKET=1`, so this exercises the guard deterministically.
    #[tokio::test]
    async fn spawn_conversation_errors_when_tddy_socket_is_unset() {
        // Given a permission server with no relay socket
        let server = PermissionServer::new();

        // When the agent calls spawn_conversation
        let result = server
            .spawn_conversation(Parameters(SpawnConversationInput {
                prompt: "Implement plans/foo.md".to_string(),
                branch: None,
                base_ref: None,
            }))
            .await;

        // Then it returns an error naming TDDY_SOCKET rather than attempting a spawn
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            parsed["error"]
                .as_str()
                .unwrap_or_default()
                .contains("TDDY_SOCKET"),
            "expected a TDDY_SOCKET error, got: {result}"
        );
    }

    /// `call_tool_by_name` (the web Inspector invoke path) dispatches a side-effect-free MCP tool
    /// in-process and returns its result; an unknown name is a clean error, not a panic.
    #[tokio::test]
    async fn call_tool_by_name_dispatches_mcp_tool_and_rejects_unknown() {
        let server = PermissionServer::new();

        // pr_stack_status runs in-process; with no TDDY_SESSION_DIR it returns its own error JSON
        // (a real result string), proving the dispatch reached the tool.
        let out = server
            .call_tool_by_name("pr_stack_status", serde_json::json!({}))
            .await
            .expect("known tool dispatches");
        assert!(
            serde_json::from_str::<serde_json::Value>(&out).is_ok(),
            "pr_stack_status must return JSON, got: {out}"
        );

        let err = server
            .call_tool_by_name("definitely_not_a_tool", serde_json::json!({}))
            .await;
        assert!(err.is_err(), "unknown tool must be an error");
    }

    /// The relayed request carries the `spawn-conversation` verb with the prompt and branch, so the
    /// daemon's `ConversationSpawnHandler` receives an explicit prompt (not a PR-stack node id).
    #[test]
    fn spawn_conversation_request_relays_the_spawn_conversation_shape() {
        // Given a prompt and an explicit branch
        let request =
            spawn_conversation_request_json("Implement plans/foo.md", Some("implement-foo"), None);

        // Then the relayed request carries the spawn-conversation verb and fields
        assert_eq!(request["type"], "spawn-conversation");
        assert_eq!(request["prompt"], "Implement plans/foo.md");
        assert_eq!(request["branch"], "implement-foo");
    }

    // -----------------------------------------------------------------------
    // Addressing a pull request, and the JSON the PR read tools put on the wire
    // -----------------------------------------------------------------------

    #[test]
    fn naming_neither_a_node_nor_a_pull_number_leaves_no_pull_request_to_read() {
        // Given / When — a read tool called with an empty address
        let result = addressed_pull_number(None, None);

        // Then — named fields, so the agent is told what to pass rather than that something was wrong
        assert_eq!(
            result,
            Err("name the pull request to read: pass node_id or pull_number".to_string())
        );
    }

    #[test]
    fn naming_both_a_node_and_a_pull_number_is_rejected_rather_than_settled_by_precedence() {
        // Given / When — the two addresses could disagree, and here they do
        let result = addressed_pull_number(Some("n1"), Some(1234));

        // Then — a precedence rule would be one more thing the agent has to know to be right about
        assert_eq!(
            result,
            Err("node_id and pull_number both name a pull request; pass exactly one".to_string())
        );
    }

    /// A read view with one review, one check run and no file list — the shape `pr_read` returns when
    /// the caller did not ask for files.
    fn a_pr_read_view() -> pr_insight::PrReadView {
        pr_insight::PrReadView {
            number: 42,
            url: "https://github.com/acme/repo/pull/42".to_string(),
            title: "Add the token store".to_string(),
            body: "Extracted from the parent PR.".to_string(),
            state: PrState::Open,
            base_branch: "master".to_string(),
            head_branch: "feature/auth/token-store".to_string(),
            head_sha: "sha-42".to_string(),
            mergeable: Some(true),
            mergeable_state: "clean".to_string(),
            additions: 10,
            deletions: 2,
            changed_files: 3,
            reviews: vec![pr_insight::ReviewerState {
                author: "alice".to_string(),
                state: "APPROVED".to_string(),
            }],
            checks: vec![CheckRun {
                name: "build".to_string(),
                conclusion: "success".to_string(),
            }],
            files: None,
        }
    }

    #[test]
    fn a_pr_read_without_files_leaves_the_files_key_out_altogether() {
        // Given — the caller did not ask for the file list, so none was fetched
        let view = a_pr_read_view();

        // When
        let json = pr_read_json(&view);

        // Then — absent, not `null`: "neither fetched nor returned" and "fetched and empty" are
        // different answers, and only an absent key says the first
        assert_eq!(json.get("files"), None);
    }

    #[test]
    fn a_pr_read_puts_the_prs_branches_on_the_wire_as_base_and_head() {
        // Given — a PR whose file list the caller did ask for
        let view = pr_insight::PrReadView {
            files: Some(vec![PrFile {
                path: "src/token_store.rs".to_string(),
                status: "added".to_string(),
            }]),
            ..a_pr_read_view()
        };

        // When
        let json = pr_read_json(&view);

        // Then — the whole agent-facing contract, `base`/`head` included: the wire names are shorter
        // than the Rust field names on purpose, and a rename would be a breaking change to the prompt
        assert_eq!(
            json,
            serde_json::json!({
                "number": 42,
                "url": "https://github.com/acme/repo/pull/42",
                "title": "Add the token store",
                "body": "Extracted from the parent PR.",
                "state": "open",
                "base": "master",
                "head": "feature/auth/token-store",
                "head_sha": "sha-42",
                "mergeable": true,
                "mergeable_state": "clean",
                "additions": 10,
                "deletions": 2,
                "changed_files": 3,
                "reviews": [{ "author": "alice", "state": "APPROVED" }],
                "checks": [{ "name": "build", "conclusion": "success" }],
                "files": [{ "path": "src/token_store.rs", "status": "added" }],
            })
        );
    }

    #[rstest]
    #[case::open(PrState::Open, "open")]
    #[case::draft(PrState::Draft, "draft")]
    #[case::merged(PrState::Merged, "merged")]
    #[case::closed(PrState::Closed, "closed")]
    fn a_prs_live_state_reaches_the_agent_in_the_lowercase_phase_vocabulary(
        #[case] state: PrState,
        #[case] expected: &str,
    ) {
        // Given / When — each state GitHub can report
        let name = pr_state_name(state);

        // Then — `draft` stays distinct from `open`: the two differ to a reviewer
        assert_eq!(name, expected);
    }

    #[test]
    fn pr_comments_puts_reviews_threads_and_conversation_on_the_wire_as_three_sections() {
        // Given — one verdict, one two-comment thread anchored to a diff line, one PR-wide comment
        let view = pr_insight::PrCommentsView {
            reviews: vec![PrReview {
                author: "alice".to_string(),
                state: "CHANGES_REQUESTED".to_string(),
                body: "Name the timeout.".to_string(),
                submitted_at: "2026-07-30T10:00:00Z".to_string(),
            }],
            threads: vec![pr_insight::PrReviewThread {
                path: "src/token_store.rs".to_string(),
                line: Some(17),
                diff_hunk: "@@ -1,3 +1,4 @@".to_string(),
                comments: vec![
                    pr_insight::PrThreadComment {
                        author: "alice".to_string(),
                        body: "Why 30?".to_string(),
                        created_at: "2026-07-30T10:01:00Z".to_string(),
                    },
                    pr_insight::PrThreadComment {
                        author: "bob".to_string(),
                        body: "The provider's own p99.".to_string(),
                        created_at: "2026-07-30T10:02:00Z".to_string(),
                    },
                ],
            }],
            conversation: vec![PrIssueComment {
                author: "carol".to_string(),
                body: "Rebased onto master.".to_string(),
                created_at: "2026-07-30T10:03:00Z".to_string(),
            }],
        };

        // When
        let json = pr_comments_json(&view);

        // Then — the whole agent-facing contract, and no `resolved` on the thread: resolution state
        // exists only on GraphQL's `reviewThreads`, so a guessed value would mislead the agent
        assert_eq!(
            json,
            serde_json::json!({
                "reviews": [{
                    "author": "alice",
                    "state": "CHANGES_REQUESTED",
                    "body": "Name the timeout.",
                    "submitted_at": "2026-07-30T10:00:00Z",
                }],
                "threads": [{
                    "path": "src/token_store.rs",
                    "line": 17,
                    "diff_hunk": "@@ -1,3 +1,4 @@",
                    "comments": [
                        { "author": "alice", "body": "Why 30?", "created_at": "2026-07-30T10:01:00Z" },
                        { "author": "bob", "body": "The provider's own p99.", "created_at": "2026-07-30T10:02:00Z" },
                    ],
                }],
                "conversation": [{
                    "author": "carol",
                    "body": "Rebased onto master.",
                    "created_at": "2026-07-30T10:03:00Z",
                }],
            })
        );
        assert_eq!(json["threads"][0].get("resolved"), None);
    }

    #[test]
    fn a_pr_search_puts_each_hit_on_the_wire_without_the_branches_github_does_not_report() {
        // Given — one open PR and one draft, as a search page returns them
        let hits = vec![
            PrSearchHit {
                number: 42,
                title: "Add the token store".to_string(),
                state: "open".to_string(),
                draft: false,
                author: "alice".to_string(),
                url: "https://github.com/acme/repo/pull/42".to_string(),
                updated_at: "2026-07-30T10:00:00Z".to_string(),
            },
            PrSearchHit {
                number: 43,
                title: "Rotate the signing key".to_string(),
                state: "open".to_string(),
                draft: true,
                author: "bob".to_string(),
                url: "https://github.com/acme/repo/pull/43".to_string(),
                updated_at: "2026-07-30T11:00:00Z".to_string(),
            },
        ];

        // When
        let json = pr_search_json(&hits);

        // Then — the whole agent-facing contract, in the order the search returned it; no `base` or
        // `head`, which GitHub's search does not report — `pr_read` is where those come from
        assert_eq!(
            json,
            serde_json::json!({
                "hits": [
                    {
                        "number": 42,
                        "title": "Add the token store",
                        "state": "open",
                        "draft": false,
                        "author": "alice",
                        "url": "https://github.com/acme/repo/pull/42",
                        "updated_at": "2026-07-30T10:00:00Z",
                    },
                    {
                        "number": 43,
                        "title": "Rotate the signing key",
                        "state": "open",
                        "draft": true,
                        "author": "bob",
                        "url": "https://github.com/acme/repo/pull/43",
                        "updated_at": "2026-07-30T11:00:00Z",
                    },
                ],
            })
        );
    }
}
