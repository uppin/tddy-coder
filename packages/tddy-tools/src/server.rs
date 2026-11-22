//! Permission server implementing the approval_prompt MCP tool and GitHub PR REST tools.

use crate::exec_tool_admission::{
    admitted_exec_tool_names, is_advertised, project_home_remote_tool_defs, stack_remote_tool_defs,
    static_exec_tool_names,
};
use crate::mcp_primitives::{
    cancel_remote_conversation, schema_object, subagent_route, RemoteToolDef,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tddy_discovery::roster::seed_subagents_or_report;
use tddy_discovery::subagent::{
    resolve_replaced_tools_for_defs, TurnRequest, SUBAGENT_MAX_TURNS_CEILING, SUBAGENT_MIN_TURNS,
};
use tddy_tool_engine::stack_git_tools::is_stack_git_tool;
use tddy_tool_engine::stack_tools::is_stack_tool;
// The conversation runtime the seven `subagent_*` tools drive: the table of open conversations, the
// turns that outlived the calls that started them, and the accounting of both. It moved to
// `tddy-discovery` with the roster at `#unbundle` node 5 — it is logic over that crate's own
// session types — while the tool bodies, their schemas and the router stayed here.
use tddy_discovery::subagent_runtime::{
    conversation_listing, pending_turn_json, report_local_conversation_state, run_turn,
    subagent_error_json, subagent_sessions, wait_for_turn, write_accounting_file, DeferredTurn,
};
use tddy_workflow_recipes::github_pr::{
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

/// The conversation a headless coding agent's permission prompts belong to: `TDDY_CONVERSATION_ID`,
/// the variable the exec tools read too.
fn permission_conversation_id() -> Option<String> {
    crate::mcp_primitives::exec_conversation_from_env()
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

/// Parameters for [`spawn_conversation`](PermissionServer::spawn_conversation): exactly
/// `StartSession`'s arguments, advertised under its schema so the agent is offered every option the
/// New-session form has (`#start-parity` 9/9).
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct SpawnConversationInput(pub tddy_tool_engine::start_session_args::StartSessionArgs);

impl schemars::JsonSchema for SpawnConversationInput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SpawnConversationInput".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::try_from(
            tddy_tool_engine::start_session_args::start_session_schema_without_choices(),
        )
        .expect("StartSession's schema is a JSON object")
    }
}

/// Parameters for [`spawn_conversation_status`](PermissionServer::spawn_conversation_status) and
/// [`spawn_conversation_cancel`](PermissionServer::spawn_conversation_cancel).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpawnConversationStartInput {
    #[schemars(description = "The start_id `spawn_conversation` returned.")]
    pub start_id: String,
}

/// Build the `start-conversation` relay request: `StartSession`'s arguments, as given. Pure so it
/// can be unit-tested without a socket.
fn start_conversation_request_json(
    args: &tddy_tool_engine::start_session_args::StartSessionArgs,
) -> serde_json::Value {
    serde_json::json!({ "type": "start-conversation", "args": args })
}

/// Build a `conversation-start-status` (`verb`) or `conversation-start-cancel` relay request.
pub(crate) fn conversation_start_request_json(verb: &str, start_id: &str) -> serde_json::Value {
    serde_json::json!({ "type": verb, "start_id": start_id })
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
    /// The conversation this server answers for, when a managed conversation launched it.
    conversation_id: Option<String>,
}

impl PermissionServer {
    pub fn new() -> Self {
        let socket_path = permission_relay_socket_path();
        let conversation_id = permission_conversation_id();
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
            // The project-home tools are registered too, so a project-home session's call to one of
            // them dispatches; `tools/list` withholds them unless the daemon admits them for the session.
            let catalog: Vec<RemoteToolDef> = exec_tool_catalog()
                .into_iter()
                .chain(project_home_remote_tool_defs())
                .filter(|tool| !replaced.contains(&tool.name))
                .collect();
            tool_router.merge(dynamic_tool_router(&catalog));
            // Session-action tools (request_action/list_actions/invoke_action). All three are host
            // round-trips — `EstablishAction`, `ListActions`, `InvokeAction` — since the session
            // directory the actions live in exists only on the host. A reachable transport is
            // therefore necessary but *not* sufficient: it says a host is there, not that the host
            // routes these three to anything. `tddy-daemon`'s handler does not
            // (`tddy_tool_engine::execute_tool_with_env` has no arm for them), so the host says so
            // itself through `TDDY_SESSION_ACTION_TOOLS` — the same shape as the `TDDY_LSP_TOOLS`
            // gate below. Nothing here reads a def's `replaces`: an action surface is not granted
            // by an agent happening to name a particular tool
            // (docs/ft/daemon/session-agent-roster.md § Tool replacement, without behaviour).
            if tddy_core::session_actions::session_action_tools_enabled() {
                tool_router.merge(crate::action_tools::action_tool_router());
            }
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
        if tddy_lsp_executor::lsp_tools::lsp_tools_enabled() {
            tool_router.merge(dynamic_tool_router(&lsp_tool_defs()));
        }
        // Restructure tools: exposed only when the host signalled (via `TDDY_RESTRUCTURE_TOOLS`)
        // that it manages a warm index to answer them; forwarded like the `Lsp*` tools.
        if tddy_core::toolcall::restructure::restructure_tools_enabled() {
            tool_router.merge(crate::restructure_tools::restructure_tool_router());
        }
        Self {
            tool_router,
            socket_path,
            conversation_id,
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
    pub fn advertised_tools(&self) -> Vec<rmcp::model::Tool> {
        self.advertised_tools_admitting(&static_exec_tool_names())
    }

    /// [`Self::advertised_tools`], with the exec catalog narrowed to the names the daemon admits for
    /// this session (see [`crate::exec_tool_admission`]). A tool outside the exec catalog is untouched.
    pub fn advertised_tools_admitting(&self, admitted: &[String]) -> Vec<rmcp::model::Tool> {
        let visible = crate::session_agents::session_agent_roster().catalog_visibility();
        let mut tools = self.tool_router.list_all();
        if !visible.has_an_agent_to_address() {
            let conversation_tools = subagent_tools::subagent_tool_names();
            tools.retain(|tool| !conversation_tools.contains(&tool.name.to_string()));
        }
        tools.retain(|tool| {
            visible
                .withdrawn_exec_tools
                .taken_over_by(tool.name.as_ref())
                .is_none()
        });
        // The one tool whose *schema* is roster-dependent: its `agent` parameter offers the ids
        // that resolve right now, which is how the main agent learns there is anyone to address.
        for tool in &mut tools {
            if tool.name == SUBAGENT_NEW_SESSION {
                tool.input_schema = subagent_new_session_schema(&visible.addressable_agents);
            }
        }
        tools.retain(|tool| is_advertised(tool.name.as_ref(), admitted));
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
        defs.extend(stack_remote_tool_defs());
        if !crate::session_agents::session_agent_roster().is_empty() {
            defs.extend(subagent_tool_router().list_all().into_iter().map(map_tool));
        }
        defs
    }

    /// Invoke one of the workflow `#[tool]` methods by name with JSON `args`, returning its result
    /// string. Used by `tddy-tools call-tool` (the web Inspector → Tools "invoke" button) to run a
    /// tool exactly as the agent would over MCP. We dispatch to the methods directly rather than via
    /// the rmcp `ToolRouter`, because a router call needs a live `Peer`/`RequestContext` that can't
    /// be fabricated outside a real MCP connection. Relay tools (`spawn_conversation`) still relay
    /// over `TDDY_SOCKET` from inside their methods; the stack plan, read and git tools are
    /// forwarded to the daemon as exec tools; the rest run in-process against
    /// `TDDY_SESSION_DIR`/`TDDY_REPO_DIR`.
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
            // Awaited rather than called like the `#[tool]` methods around it: this one is an
            // `async fn`, and it is safe on the Inspector's invoke path because it is a pure read
            // of process-local state under a deadline it caps itself.
            "subagent_status" => subagent_status_tool(args).await,
            // The plan and read tools are the daemon's exec tools, addressed by stack id
            // (`#orchestrators` 7/11): forwarded to it like any other exec tool, never run here.
            stack_tool if is_stack_tool(stack_tool) => {
                dispatch_dynamic_tool(stack_tool, args).await
            }
            // The git tools are the daemon's exec tools too (`#orchestrators` 10/11): forwarded
            // to it like any other exec tool, never run here.
            git_tool if is_stack_git_tool(git_tool) => dispatch_dynamic_tool(git_tool, args).await,
            "spawn_conversation" => self.spawn_conversation(Parameters(parse(args)?)).await,
            "spawn_conversation_status" => {
                self.spawn_conversation_status(Parameters(parse(args)?))
                    .await
            }
            "spawn_conversation_cancel" => {
                self.spawn_conversation_cancel(Parameters(parse(args)?))
                    .await
            }
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

    /// Decide a permission prompt exactly as the `approval_prompt` tool does. Plain delegation to
    /// [`decide`](Self::decide), for callers outside this module.
    pub fn decide_for_approval(&self, tool_name: &str, input: &Value) -> String {
        self.decide(tool_name, input)
    }

    /// Decide allow/deny. Bash(tddy-tools *) and mcp__tddy-tools__* are always allowed.
    /// Bash commands that only reference paths under TDDY_SESSION_DIR or TDDY_REPO_DIR are pre-allowed.
    /// For other tools: route through TDDY_SOCKET to TUI if available, else deny.
    /// A server answering for a conversation (`TDDY_CONVERSATION_ID`) hands the prompt to that
    /// conversation instead; see [`Self::decide_for_conversation`].
    ///
    /// Claude Code permission-prompt-tool expects allow responses to include `updatedInput` (the
    /// original or modified tool input). Deny responses use `behavior: "deny"` and optional `message`.
    fn decide(&self, tool_name: &str, input: &Value) -> String {
        if let Some(conversation_id) = self.conversation_id.as_deref() {
            return self.decide_for_conversation(conversation_id, tool_name, input);
        }
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

    /// Decide a prompt raised by the conversation `conversation_id`. The conversation's own decision
    /// point answers, over the tool route (`RequestApproval`), and no local pre-allow applies: a
    /// headless conversation yields every prompt to its conversation. tddy-tools' own MCP tools are
    /// the one exception.
    ///
    /// Fails closed: only an explicit allow from the daemon is an allow. A refusal, a missing route, a
    /// call that fails and a daemon that does not answer in time are denials, each carrying its reason.
    fn decide_for_conversation(
        &self,
        conversation_id: &str,
        tool_name: &str,
        input: &Value,
    ) -> String {
        let allow_response = || serde_json::json!({ "behavior": "allow", "updatedInput": input });
        if tool_name.starts_with("mcp__tddy-tools__") {
            return allow_response().to_string();
        }
        let deny = |message: String| {
            serde_json::json!({ "behavior": "deny", "message": message }).to_string()
        };
        match Self::request_conversation_approval(conversation_id, tool_name, input) {
            Ok(true) => allow_response().to_string(),
            Ok(false) => deny(format!("Permission denied for {tool_name}")),
            Err(reason) => deny(reason),
        }
    }

    /// One conversation-tagged approval over the tool route, asked through the session tool client.
    /// The round trip runs on a thread with a runtime of its own, so no runtime is entered from inside
    /// the caller's, and it is bounded by the client's approval bound: a route that never answers
    /// denies the prompt.
    ///
    /// `Ok(allow)` is the daemon's decision; `Err` carries the text a denial reports.
    fn request_conversation_approval(
        conversation_id: &str,
        tool_name: &str,
        input: &Value,
    ) -> Result<bool, String> {
        let round_trip = std::thread::scope(|scope| {
            scope
                .spawn(move || -> Result<bool, String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| format!("could not start the approval runtime: {e}"))?;
                    runtime.block_on(async {
                        match tokio::time::timeout(
                            crate::session_tool_client::APPROVAL_CLIENT_BOUND,
                            crate::session_tool_client::request_approval(
                                conversation_id,
                                tool_name,
                                input,
                            ),
                        )
                        .await
                        {
                            Ok(decision) => decision,
                            Err(_elapsed) => Err(format!(
                                "the approval from conversation {conversation_id} timed out"
                            )),
                        }
                    })
                })
                .join()
        });
        match round_trip {
            Ok(decision) => decision,
            Err(_panicked) => Err(format!(
                "the approval from conversation {conversation_id} failed unexpectedly"
            )),
        }
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
        description = "Create a GitHub pull request (REST POST /repos/{owner}/{repo}/pulls). Authenticates as the project's assigned GitHub account; uses curl against api.github.com."
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
        let token = match github_token_for_agent() {
            Ok(token) => token,
            Err(reason) => return serde_json::json!({ "error": reason }).to_string(),
        };
        match create_pull_request_via_rest_api(&params, &token) {
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
        description = "Update an existing GitHub pull request metadata (REST PATCH). Authenticates as the project's assigned GitHub account."
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
        let token = match github_token_for_agent() {
            Ok(token) => token,
            Err(reason) => return serde_json::json!({ "error": reason }).to_string(),
        };
        match update_pull_request_via_rest_api(&params, &token) {
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
        description = "Start a session — any type, on this host or another eligible one, with any option the New-session form offers — in this session's own project, tagged with the current session as its orchestrator. Returns at once with start_id, task_id and daemon_instance_id: follow it with spawn_conversation_status, stop it with spawn_conversation_cancel. A start the form would refuse outright (a CLI session without a model, a branch another session owns) is refused now, naming why."
    )]
    async fn spawn_conversation(
        &self,
        Parameters(p): Parameters<SpawnConversationInput>,
    ) -> String {
        // Relay to the daemon over the per-session TDDY_SOCKET. The daemon runs the same start a
        // `StartSession` call gets, linked to this session — available to any managed session that
        // can spawn conversations (e.g. grill-me).
        relay_to_daemon("spawn_conversation", start_conversation_request_json(&p.0)).await
    }

    #[tool(
        description = "Read a conversation's start by start_id: its state (running, succeeded, failed or cancelled), the step it is in, the clone's progress, and once it succeeded its session_id, branch and worktree, or once it failed or was cancelled, why."
    )]
    async fn spawn_conversation_status(
        &self,
        Parameters(p): Parameters<SpawnConversationStartInput>,
    ) -> String {
        relay_to_daemon(
            "spawn_conversation_status",
            conversation_start_request_json("conversation-start-status", &p.start_id),
        )
        .await
    }

    #[tool(
        description = "Cancel a conversation's start by start_id while it is running, on whichever host runs it. It returns at once; the start reads cancelled once it has stopped, and a session it had already created is removed."
    )]
    async fn spawn_conversation_cancel(
        &self,
        Parameters(p): Parameters<SpawnConversationStartInput>,
    ) -> String {
        relay_to_daemon(
            "spawn_conversation_cancel",
            conversation_start_request_json("conversation-start-cancel", &p.start_id),
        )
        .await
    }
}

/// Send `request` to the daemon over the per-session `TDDY_SOCKET` and return its answer, or the
/// refusal as `{"error": …}`.
async fn relay_to_daemon(tool: &str, request: serde_json::Value) -> String {
    let Some(socket) = permission_relay_socket_path() else {
        return serde_json::json!({
            "error": format!("TDDY_SOCKET is not set; {tool} requires a managed session")
        })
        .to_string();
    };
    match tddy_core::toolcall::dispatch_toolcall(&socket, request).await {
        Ok(resp) => resp.to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

// ---------------------------------------------------------------------------
// PR-stack tool helpers
// ---------------------------------------------------------------------------

/// The GitHub credential the agent's PR tools authenticate with: the token of the account the
/// session's project acts as, asked of the session's host over `TDDY_SOCKET` at call time.
///
/// Not an environment variable and not a file — see [`crate::github_credential`]. `Err` is the
/// host's own refusal (or the absence of a host), returned to the agent as the tool's error.
fn github_token_for_agent() -> Result<String, String> {
    crate::github_credential::github_token_for_session()
}

// Explicit `router = self.tool_router` — the default `#[tool_handler]` expansion calls the
// static `Self::tool_router()` (the macro-generated router only), which would silently ignore
// any dynamic tools merged into the instance's `self.tool_router` field by `PermissionServer::new()`.
#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for PermissionServer {
    fn get_info(&self) -> ServerInfo {
        // `enable_tool_list_changed` is what makes the roster's live catalog reach the client at
        // all: a client that was not told the list can change may ignore
        // `notifications/tools/list_changed`, and Claude Code does — so without this declaration an
        // agent attached after the handshake never appears in the tool list the client read once,
        // and the exec tools it replaced are never taken away.
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_instructions(
            "Permission prompt tool for tddy-coder. Denies unexpected tool requests. \
             This server also exposes GitHub PR tools, which authenticate as the project's assigned GitHub account: \
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
        // Asked per listing, not once: a project-home session's role is recorded as the session starts,
        // and the daemon's answer is the one the session's agent is offered. A failed ask is refused
        // here, as an MCP error the operator sees, rather than answered with a catalog of this crate's own.
        let admitted = match admitted_exec_tool_names().await {
            Ok(names) => names,
            Err(reason) => {
                log::error!(target: "tddy_tools::server", "{reason}");
                return Err(rmcp::ErrorData::internal_error(reason, None));
            }
        };
        Ok(rmcp::model::ListToolsResult {
            tools: self.advertised_tools_admitting(&admitted),
            meta: None,
            next_cursor: None,
        })
    }
}

// --- Remote-codebase mode: dynamic tool catalog helpers ---

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

/// Dispatch a call to a dynamic (non-static) tool via the session daemon.
///
/// Uses [`crate::session_tool_client::dispatch_session_tool`] — sandbox IPC when
/// `TDDY_SANDBOX_TOOL_IPC` is set, otherwise HTTP to `TDDY_REMOTE_DAEMON_URL`. When
/// `TDDY_CONVERSATION_ID` names the conversation a managed coding agent runs on,
/// [`crate::session_tool_client::dispatch_conversation_tool`] instead, so the daemon runs the call
/// in that conversation's worktree.
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
    match crate::mcp_primitives::exec_conversation_from_env() {
        Some(conversation) => {
            crate::session_tool_client::dispatch_conversation_tool(&conversation, tool_name, args)
                .await
        }
        None => crate::session_tool_client::dispatch_session_tool(tool_name, args).await,
    }
}

/// The exec tools forwarded to Claude Code when a session-tool transport (sandbox IPC or daemon
/// HTTP) is configured, in the MCP shape.
///
/// There is one catalog and it is [`tddy_tool_engine::tool_catalog`] — the crate that executes
/// these tools also defines them. This function is the single place their `ToolDef` is mapped to
/// the `RemoteToolDef` the MCP router advertises, exactly as [`lsp_tool_defs`] maps
/// `tddy_lsp_executor`'s. Until `#unbundle` node 5 the ten entries were hand-copied here and kept
/// in step by a guard test on each side; the copy is gone, so the names cannot drift.
///
/// The per-session answer is narrowed onto this catalog at `tools/list` time (see
/// [`crate::exec_tool_admission`]). The daemon's session-scoped `ListExecTools` answer is asked over
/// every transport (HTTP, unix socket, the sandbox socket and LiveKit), and a failed ask refuses
/// `tools/list`.
pub fn exec_tool_catalog() -> Vec<RemoteToolDef> {
    tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|tool| RemoteToolDef {
            name: tool.name,
            description: tool.description,
            input_schema_json: tool.input_schema_json,
        })
        .collect()
}

/// The five language-agnostic LSP operations as MCP tool defs.
///
/// `tddy-lsp-executor` owns the operations, their descriptions and their argument schemas — it is
/// what answers the calls. The MCP shape they are advertised in belongs here, which is the same
/// split `mcp_primitives` documents: what leaves this crate is a tool's implementation, not the
/// shape of an MCP tool.
fn lsp_tool_defs() -> Vec<RemoteToolDef> {
    tddy_lsp_executor::lsp_tools::lsp_tool_catalog()
        .into_iter()
        .map(|tool| RemoteToolDef {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            input_schema_json: tool.input_schema_json.to_string(),
        })
        .collect()
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
/// The conversation tool whose `agent` parameter is answered from the live roster.
const SUBAGENT_NEW_SESSION: &str = "subagent_new_session";

mod subagent_tools;
pub use subagent_tools::*;

/// Parse the `replacement` argument of a resume, or why it cannot run as written.
///
/// Rejected **before** the turn runs, like the yield conditions and for the same reason: the
/// replacement is the caller's own words, and a resume that continues without it does less than
/// was asked.
fn replacement_of(
    args: &serde_json::Value,
    mut request: TurnRequest,
) -> Result<TurnRequest, String> {
    let Some(raw) = args.get("replacement") else {
        return Ok(request);
    };
    let encoded = serde_json::to_string(raw).map_err(|e| format!("replacement: {e}"))?;
    let replacement = serde_json::from_str::<tddy_discovery::subagent::Replacement>(&encoded)
        .map_err(|e| format!("replacement is not a {{tool, arguments, result}} object: {e}"))?;
    tddy_discovery::subagent::validate_replacement(&replacement)?;
    request = request.with_replacement(replacement);
    Ok(request)
}

/// Apply a caller's `maxTurns` to `request`, or say why the value cannot be read.
///
/// Refused naming the field rather than ignored, for the reason [`blocking_budget`] refuses a
/// malformed budget: a turn budget silently dropped has the caller reading an early stop as a
/// finished search. A value above the ceiling is **not** refused — it is clamped by the session
/// and the clamp comes back in the outcome's `clampedMaxTurns`.
fn turn_budget(args: &serde_json::Value, request: TurnRequest) -> Result<TurnRequest, String> {
    match args.get("maxTurns") {
        None | Some(serde_json::Value::Null) => Ok(request),
        Some(value) => match value.as_u64().and_then(|turns| u32::try_from(turns).ok()) {
            Some(max_turns) => Ok(request.within_turns(max_turns)),
            None => Err(format!(
                "maxTurns must be a whole number of model turns to spend on this call, got: {value}"
            )),
        },
    }
}

/// Run one turn of an open conversation, whatever the turn was asked to do.
///
/// The body both conversation tools share, because everything after "what is this turn" is the
/// same: the conversation is looked up on the live roster, the turn is spawned so it survives its
/// caller, and the call blocks for at most `grace` before handing back a receipt. Two copies of
/// this would be two chances for a resume to queue, refuse or report unlike a prompt.
///
/// A conversation whose agent was detached underneath it is refused naming the detach rather than
/// prompted: the main agent waits on this call, so a conversation that can no longer be answered
/// has to fail rather than hang.
async fn take_a_turn(session_id: &str, request: TurnRequest, grace: std::time::Duration) -> String {
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
            let remote = sessions
                .open
                .get(session_id)
                .and_then(|conv| conv.remote.clone());
            sessions.retire(session_id);
            // Anyone parked on a turn of this conversation is answered here: the conversation is
            // gone, so no turn of it will ever produce an outcome to wait for.
            sessions.pending.cancel_conversation(session_id, &reason);
            // The roster tracked this conversation only so it could be cancelled; nothing will ask
            // about it again, and a cancelled entry nobody forgets is rescanned by every later
            // frame for the process lifetime.
            roster.close_conversation(session_id);
            write_accounting_file(&sessions);
            drop(sessions);
            cancel_remote_conversation(remote).await;
            return subagent_error_json(reason);
        }
    }
    let Some(conv) = sessions.open.get(session_id) else {
        return subagent_error_json(format!("unknown subagent session: {session_id}"));
    };
    let awaiting_user = request.permission_deferred_to_user().is_some();
    let turn = DeferredTurn {
        response_id: uuid::Uuid::new_v4().to_string(),
        conversation_id: session_id.to_string(),
        request,
        session: conv.session.clone(),
        // Only a loop running here is reported on; one the daemon runs it already sees.
        reported_agent: conv.remote.is_none().then(|| conv.agent.clone()),
    };
    let response_id = turn.response_id.clone();
    // Read before the spawn takes the conversation's session: the turn queues on its endpoint as
    // well as on its conversation, and the receipt its caller gets reports both.
    let provider = conv.provider().map(str::to_string);
    // Spawned and registered under the same hold on the table: the task takes this lock before it
    // can publish anything, so registering it here cannot lose a race with a turn that finishes
    // immediately.
    let running = tokio::spawn(run_turn(turn));
    sessions.pending.start(
        &response_id,
        session_id,
        provider.as_deref(),
        running.abort_handle(),
    );
    if awaiting_user {
        sessions.pending.mark_awaiting_user(&response_id);
    }
    let mut watched = sessions
        .pending
        .watch(&response_id)
        .expect("a turn registered under this lock must be watchable");
    drop(sessions);

    match wait_for_turn(&mut watched, grace).await {
        // Its caller was never told the id, so nothing can ever name it: keeping the entry would be
        // a per-turn leak with no reader.
        Some(result) => {
            subagent_sessions()
                .lock()
                .await
                .pending
                .forget(&response_id);
            crate::pull_ledger::annotated_turn_result(session_id, &response_id, result)
        }
        // The queue is read under a fresh hold on the table, after the grace period rather than
        // before it: the caller is told what it is waiting behind now, not what it was waiting
        // behind when the prompt was accepted.
        None => {
            let sessions = subagent_sessions().lock().await;
            pending_turn_json(&sessions.pending, &response_id)
        }
    }
}

/// `subagent_await`: collect the outcome of a turn that outran its prompt's grace period.
///
/// An unknown id is refused naming it rather than waited on — an await that blocked out its timeout
/// on an id nothing was ever stored under is indistinguishable from a slow turn, and would have the
/// agent poll forever for an answer that is not coming (criterion 29).
pub async fn subagent_await_tool(args: serde_json::Value) -> String {
    let Some(response_id) = args.get("responseId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: responseId");
    };
    let timeout = match subagent_tools::blocking_budget(
        &args,
        "timeoutMs",
        subagent_tools::SUBAGENT_PROMPT_GRACE,
    ) {
        Ok(timeout) => timeout,
        Err(e) => return subagent_error_json(e),
    };
    let watched = subagent_sessions().lock().await.pending.watch(response_id);
    let Some(mut watched) = watched else {
        return subagent_error_json(format!("unknown subagent response: {response_id}"));
    };
    match wait_for_turn(&mut watched, timeout).await {
        Some(result) => {
            let conversation = subagent_sessions()
                .lock()
                .await
                .pending
                .conversation_of(response_id)
                .map(str::to_string);
            match conversation {
                Some(conversation) => {
                    crate::pull_ledger::annotated_turn_result(&conversation, response_id, result)
                }
                None => result,
            }
        }
        // Read after the wait, so a caller polling a queued turn sees its position fall as the
        // turns accepted ahead of it end — the question an await repeated on one id is asking.
        None => {
            let sessions = subagent_sessions().lock().await;
            pending_turn_json(&sessions.pending, response_id)
        }
    }
}

/// `subagent_cancel` (ACP `session/cancel`-shaped): cancels an open conversation's running turn.
///
/// A conversation whose loop runs on the daemon is asked to cancel only its turn; the daemon keeps a coding agent's
/// conversation open (its `session/cancel`), and the conversation stays open here too, answering
/// `{cancelled: true, kept: true}`. Every other conversation — one the daemon closed, one whose loop runs here — is
/// closed, as [`subagent_close_tool`] closes it. A daemon that cannot be asked leaves the conversation open, and the
/// error is the answer.
pub async fn subagent_cancel_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    // Read under the table's lock and asked without it: the daemon answers once the cancelled turn has ended, and
    // a lock held that long would hold every other subagent tool behind it.
    let remote = subagent_sessions()
        .lock()
        .await
        .open
        .get(session_id)
        .and_then(|conv| conv.remote.clone());
    let Some(remote) = remote else {
        return close_conversation(session_id, None).await;
    };
    match remote.cancel_turn().await {
        Ok(true) => serde_json::json!({ "cancelled": true, "kept": true }).to_string(),
        // The daemon closed it: closed here too, with nothing left to tell the daemon.
        Ok(false) => close_conversation(session_id, None).await,
        // Nothing was cancelled, so nothing is closed in its place: the conversation stays open on both sides, and the
        // main agent is told why.
        Err(e) => subagent_error_json(format!(
            "conversation '{session_id}' could not have its turn cancelled: {e}"
        )),
    }
}

/// Close `sessionId`'s conversation, on the daemon running its loop as well when it runs on one — what
/// `subagent_end` does once its work is taken, and what `subagent_cancel` does for a conversation that is not kept.
pub(crate) async fn subagent_close_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let remote = subagent_sessions()
        .lock()
        .await
        .open
        .get(session_id)
        .and_then(|conv| conv.remote.clone());
    close_conversation(session_id, remote).await
}

/// Close `session_id` here, and on the daemon through `remote` when it is given.
async fn close_conversation(
    session_id: &str,
    remote: Option<crate::session_agents::RemoteConversationHandle>,
) -> String {
    let mut sessions = subagent_sessions().lock().await;
    // The agent behind a loop running *here*, read before the retire consumes the conversation. A
    // remote one needs no report: cancelling it goes through the daemon, which records it itself.
    let reported_agent = sessions
        .open
        .get(session_id)
        .filter(|conv| conv.remote.is_none())
        .map(|conv| conv.agent.clone());
    // Retired rather than forgotten: a cancelled conversation's tokens were spent by this session,
    // and the accounting file below is rewritten from this table wholesale.
    let cancelled = sessions.retire(session_id);
    // Every turn of it is answered as cancelled: an await already parked on one would otherwise
    // wait out a turn nobody will ever finish (criterion 31).
    sessions
        .pending
        .cancel_conversation(session_id, "conversation cancelled");
    // The roster tracks the same conversation, so that a later detach of its agent does not report
    // a cancellation for something the main agent already closed.
    crate::session_agents::session_agent_roster().close_conversation(session_id);
    write_accounting_file(&sessions);
    drop(sessions);
    cancel_remote_conversation(remote).await;
    // After the retire, so nothing can prompt a conversation whose worktree is going away. Only a
    // conversation whose loop runs here has one: a daemon-run conversation never creates it.
    if cancelled && reported_agent.is_some() {
        crate::subagent_end::discard_conversation_worktree(session_id).await;
    }
    if let Some(agent_id) = reported_agent.as_deref() {
        // Idle rather than left mid-turn: the conversation is gone, so a row still reporting a turn
        // in flight would be one nothing can ever finish.
        report_local_conversation_state(
            agent_id,
            tddy_service::proto::types::SessionAgentStatus::Idle,
            "conversation cancelled",
        )
        .await;
    }
    serde_json::json!({ "cancelled": cancelled }).to_string()
}

/// `subagent_list`: enumerate every conversation this session ran — open and ended — with its
/// per-conversation token accounting and, for the open ones, how full its context is.
async fn subagent_list_tool(_args: serde_json::Value) -> String {
    let sessions = subagent_sessions().lock().await;
    serde_json::json!({ "conversations": conversation_listing(&sessions) }).to_string()
}

/// A roster status as the word the tool reports, and the `data-` value a UI would key on.
///
/// UNSPECIFIED is reported as `"unknown"` rather than `"idle"`, and the distinction is the whole
/// reason this tool exists: an agent nothing has been observed of is not an agent that is free, and
/// telling the main agent it is idle would have it dispatch work to a row the daemon cannot
/// currently account for.
fn agent_status_word(status: i32) -> &'static str {
    use tddy_service::proto::types::SessionAgentStatus;
    match SessionAgentStatus::try_from(status) {
        Ok(SessionAgentStatus::Idle) => "idle",
        Ok(SessionAgentStatus::Running) => "running",
        Ok(SessionAgentStatus::ExecutingTool) => "executing_tool",
        Ok(SessionAgentStatus::WaitingForInput) => "waiting_for_input",
        Ok(SessionAgentStatus::Connecting) => "connecting",
        Ok(SessionAgentStatus::Error) => "error",
        Ok(SessionAgentStatus::Unspecified) | Err(_) => "unknown",
    }
}

/// The clone state as a word. `local` is a state of its own, not a synonym for `ready`: there is no
/// checkout behind a local agent, so saying "ready" would imply one exists.
fn clone_state_word(state: i32) -> &'static str {
    use tddy_service::proto::session_agents_svc::AgentCloneState;
    match AgentCloneState::try_from(state) {
        Ok(AgentCloneState::Local) => "local",
        Ok(AgentCloneState::Provisioning) => "provisioning",
        Ok(AgentCloneState::Ready) => "ready",
        Ok(AgentCloneState::Error) => "error",
        Ok(AgentCloneState::Unspecified) | Err(_) => "unknown",
    }
}

/// One roster row as the JSON `subagent_status` reports.
fn agent_status_json(agent: &crate::session_agents::AgentStatus) -> serde_json::Value {
    let entry = &agent.entry;
    let mut row = serde_json::json!({
        "agentId": entry.agent_id,
        "label": entry.label,
        "model": entry.model,
        "daemonInstanceId": entry.daemon_instance_id,
        "status": agent_status_word(entry.status),
        "cloneState": clone_state_word(entry.clone_state),
        "replaces": entry.replaces,
        "conversations": agent
            .conversations
            .iter()
            .map(|c| match &c.cancelled_reason {
                None => serde_json::json!({ "id": c.conversation_id, "state": "open" }),
                Some(reason) => serde_json::json!({
                    "id": c.conversation_id,
                    "state": "cancelled",
                    "reason": reason,
                }),
            })
            .collect::<Vec<_>>(),
    });
    // Absent rather than empty when nothing has been observed: `"lastActivity": {}` reads as an
    // agent that did something unnameable, which is a different claim from "nothing yet".
    if let Some(activity) = &entry.last_activity {
        row["lastActivity"] = serde_json::json!({
            "atUnixMs": activity.at_unix_ms,
            "summary": activity.summary,
        });
    }
    // Only when there is one — an empty string on every healthy row is noise the main agent has to
    // read past on every call.
    if !entry.clone_error.is_empty() {
        row["cloneError"] = entry.clone_error.clone().into();
    }
    row
}

/// The only condition `waitFor` accepts. An open-ended string would let the main agent ask to wait
/// for something nothing produces, and spend its whole deadline learning nothing.
const WAIT_FOR_READY: &str = "ready";

/// How long `subagent_status { waitFor }` parks when the caller names no deadline.
const SUBAGENT_STATUS_DEFAULT_WAIT_MS: u64 = 30_000;

/// The longest it parks whatever the caller names. One tool call holding the main agent for ten
/// minutes is worse than one returning "still connecting" and letting it decide.
const SUBAGENT_STATUS_MAX_WAIT_MS: u64 = 120_000;

/// Whether waiting on this status could still change anything.
///
/// Promptable is `status ∉ { CONNECTING, ERROR }` — the two states in which a prompt is refused —
/// but only one of them is worth waiting through, so this is the wait's condition rather than
/// promptability itself.
fn readiness_is_settled(status: i32) -> bool {
    use tddy_service::proto::types::SessionAgentStatus as Status;
    match Status::try_from(status) {
        // The one state a later frame is expected to move out of: the checkout behind the agent is
        // still being built, which is precisely what the caller asked to be told about.
        Ok(Status::Connecting) => false,
        // A failed checkout is not something waiting will fix. The wait ends and reports the state,
        // so the main agent can say why it gave up — rather than spending the whole deadline to
        // report the same failure later, and as a timeout.
        Ok(Status::Error) => true,
        // Promptable. UNSPECIFIED counts, the unnamed discriminant of a newer daemon included: it is
        // what a roster restored from `.session.yaml` honestly reports for an agent whose checkout
        // is on disk and perfectly promptable, so treating it as not-ready would park every wait on
        // a restarted daemon until its deadline.
        _ => true,
    }
}

/// The tool result for `report`, with `timedOut` only when a wait was actually asked for.
///
/// A field that is always `false` on a call that could not time out reads as a guarantee about a
/// wait that never happened, so a plain read carries no `timedOut` at all.
fn status_report_json(
    report: &crate::session_agents::RosterStatusReport,
    timed_out: Option<bool>,
) -> String {
    let mut payload = serde_json::json!({
        "sessionId": report.session_id,
        "agents": report.agents.iter().map(agent_status_json).collect::<Vec<_>>(),
    });
    // `null` rather than omitted: "no frame has ever arrived" is a state the caller should be able
    // to read off the field, not infer from its absence.
    payload["appliedRev"] = match report.applied_rev {
        Some(rev) => rev.into(),
        None => serde_json::Value::Null,
    };
    if let Some(refusal) = &report.refusal {
        payload["refusal"] = refusal.clone().into();
    }
    if let Some(timed_out) = timed_out {
        payload["timedOut"] = timed_out.into();
    }
    payload.to_string()
}

/// `subagent_status`: what every attached agent is doing right now.
///
/// Distinct from `subagent_list`, which enumerates this process's *conversations* and their token
/// cost. This reports the **roster**: agents the main agent could address but has never opened a
/// conversation with appear here and nowhere else, which is what makes it answerable before any
/// work has been dispatched.
///
/// A roster that has gone stale still answers, carrying `refusal`: the rows it last knew about plus
/// the reason none of them can be addressed is strictly more useful than an error, which would
/// answer "what are my agents doing?" with nothing at all.
async fn subagent_status_tool(args: serde_json::Value) -> String {
    let roster = crate::session_agents::session_agent_roster();
    let agent = args
        .get("agent")
        .and_then(|value| value.as_str())
        .filter(|agent| !agent.is_empty());
    let wait_for = args
        .get("waitFor")
        .and_then(|value| value.as_str())
        .filter(|condition| !condition.is_empty());

    let Some(wait_for) = wait_for else {
        return status_report_json(&roster.status_report(), None);
    };
    if wait_for != WAIT_FOR_READY {
        return subagent_error_json(format!(
            "'waitFor' takes only \"{WAIT_FOR_READY}\"; got '{wait_for}'"
        ));
    }
    // Readiness across an unbounded roster is neither "all of them" nor "any of them" in a way the
    // caller could have meant, and guessing either would make the answer depend on attach order —
    // which the main agent cannot see. So it names the field it needs rather than picking one.
    let Some(agent) = agent else {
        return subagent_error_json(format!(
            "'waitFor' requires 'agent': waiting for \"{WAIT_FOR_READY}\" across every attached \
             agent is neither \"all of them\" nor \"any of them\", and either reading would make \
             the answer depend on attach order"
        ));
    };
    let wait = match args.get("timeoutMs") {
        None | Some(serde_json::Value::Null) => SUBAGENT_STATUS_DEFAULT_WAIT_MS,
        Some(value) => match value.as_u64() {
            Some(timeout_ms) => timeout_ms.min(SUBAGENT_STATUS_MAX_WAIT_MS),
            // Refused rather than quietly replaced by the default: a caller that asked for a
            // deadline and silently got a different one reads the eventual timeout as its own.
            None => {
                return subagent_error_json(format!(
                    "'timeoutMs' is a whole number of milliseconds; got {value}"
                ))
            }
        },
    };
    wait_until_readiness_is_settled(agent, std::time::Duration::from_millis(wait)).await
}

/// Park until `agent` can be prompted, the deadline passes, or the roster stops listing it.
///
/// Every ending returns the **same report** a plain read returns, plus `timedOut`. Nothing here
/// errors on the roster's account, which is the property `status_report` already holds: an agent
/// detached mid-wait is simply absent from `agents`, and a roster gone dark carries `refusal` — in
/// both cases the report says what happened better than an error naming it would, and still carries
/// every other row.
async fn wait_until_readiness_is_settled(agent: &str, wait: std::time::Duration) -> String {
    let roster = crate::session_agents::session_agent_roster();
    let deadline = tokio::time::Instant::now() + wait;
    // Subscribed *before* the first read: a frame landing between a read and the park is already
    // counted against this receiver's version, so the park returns at once rather than sleeping
    // through the very transition it is waiting for.
    let mut snapshots = roster.subscribe_to_snapshots();
    let mut seen_on_the_roster = false;
    loop {
        let report = roster.status_report();
        let listed = report
            .agents
            .iter()
            .find(|candidate| candidate.entry.agent_id == agent);
        // An id no row has *ever* borne is a caller error, not a roster state, and it is refused the
        // way `subagent_new_session` refuses one — naming the ids there are. Settling it silently
        // would answer a misspelled agent with `timedOut: false`, which reads as "it is ready".
        // Once the id has been seen, the same absence means a detach: there the wait settles, since
        // what it is waiting for cannot happen any more, and the report says so by not listing it.
        if listed.is_none() && !seen_on_the_roster && report.refusal.is_none() {
            return subagent_error_json(format!(
                "agent '{agent}' is not attached to this session. Attached agents: [{}]",
                report
                    .agents
                    .iter()
                    .map(|candidate| candidate.entry.agent_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        seen_on_the_roster |= listed.is_some();
        let still_connecting =
            listed.is_some_and(|candidate| !readiness_is_settled(candidate.entry.status));
        if !still_connecting || report.refusal.is_some() {
            return status_report_json(&report, Some(false));
        }
        tokio::select! {
            woken = snapshots.changed() => {
                if woken.is_err() {
                    return subagent_error_json(format!(
                        "the session's agent roster stopped publishing, so agent '{agent}' \
                         becoming ready can no longer be observed"
                    ));
                }
            }
            // Expiry is not an error: the last known status is the actionable half of the answer,
            // and an error throws it away — "still connecting" is something the main agent can act
            // on, and it is exactly what the rows already say.
            _ = tokio::time::sleep_until(deadline) => {
                return status_report_json(&report, Some(true));
            }
        }
    }
}

/// The `subagent_new_session` input schema for a session whose roster holds `agents`.
///
/// The `agent` choices are the whole reason this is a function of the roster rather than a constant.
/// An id is matched whole (`LiveAgentRoster::resolve`), `subagent_list` reports open *conversations*
/// rather than attached agents, and nothing else enumerates the roster — so an attached agent this
/// schema does not name is one the main agent cannot address, however faithfully the catalog
/// followed the roster. Rebuilt per `tools/list` from
/// [`PermissionServer::advertised_tools`], which the client re-reads on every applied revision.
pub(crate) fn subagent_new_session_schema(
    agents: &[crate::session_agents::AddressableAgent],
) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    let mut agent = serde_json::json!({
        "type": "string",
        "description": agent_parameter_description(agents),
    });
    if !agents.is_empty() {
        agent["enum"] = agents
            .iter()
            .map(|agent| agent.agent_id.clone())
            .collect::<Vec<_>>()
            .into();
    }
    schema_object(serde_json::json!({
        "type": "object",
        "properties": {
            "agent": agent,
            "sessionId": {"type": "string", "description": "Caller-chosen conversation id. Generated if omitted."},
            "cwd": {"type": "string", "description": "Optional working directory hint."},
            "systemPrompt": {
                "type": "string",
                "description": "Optional system prompt for this conversation. It REPLACES the \
                                agent's own system prompt rather than appending to it, and only \
                                for this conversation — omit it to keep the agent's. Use it to \
                                tell the agent what its definition could not know: which \
                                repository this is, what shape the answer must take, or what it \
                                must stop doing after a turn that went wrong."
            }
        }
    }))
}

/// What the `agent` parameter says it takes: the ids that resolve, spelled out.
///
/// Spelled out in prose as well as in the `enum` because a client that renders a schema without its
/// `enum` would otherwise show a free-text field and no way to learn what goes in it.
fn agent_parameter_description(agents: &[crate::session_agents::AddressableAgent]) -> String {
    if agents.is_empty() {
        return "Required. The qualified id of a specialized agent attached to this session."
            .to_string();
    }
    let choices: Vec<String> = agents
        .iter()
        .map(|agent| {
            let mut about = Vec::new();
            if !agent.label.is_empty() {
                about.push(agent.label.clone());
            }
            if !agent.daemon_instance_id.is_empty() {
                about.push(format!("on {}", agent.daemon_instance_id));
            }
            match about.is_empty() {
                true => agent.agent_id.clone(),
                false => format!("{} ({})", agent.agent_id, about.join(", ")),
            }
        })
        .collect();
    format!(
        "Required. One of the agents attached to this session: {}.",
        choices.join("; ")
    )
}

/// The `subagent_prompt` input schema. Named, like [`subagent_new_session_schema`], so the router
/// reads as the list of tools it is rather than the schemas they carry.
fn subagent_prompt_schema() -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
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
            },
            "graceMs": {
                "type": "integer",
                "minimum": 0,
                "description": "How long to block for the turn before returning a responseId to \
                                collect it with. Defaults to 25000; 0 defers immediately."
            },
            "maxTurns": max_turns_property(),
            "yieldConditions": yield_conditions_property(),
            (crate::sync_worktree_choice::SYNC_WORKTREE_ARG): crate::sync_worktree_choice::sync_worktree_property(),
        }
    }))
}

/// The `subagent_resume` input schema: the conversation is required, everything that reshapes the
/// turn is not.
fn subagent_resume_schema() -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    schema_object(serde_json::json!({
        "type": "object",
        "required": ["sessionId"],
        "properties": {
            "sessionId": {"type": "string"},
            "fromMessageId": {
                "type": "string",
                "description": "Send the conversation back to this message first, discarding \
                                everything after it. The ids come from a turn outcome's \
                                `messages`. A rewind landing between a tool call and its results \
                                keeps the results, so the history stays one the model accepts. An \
                                id this conversation does not hold is an error, never a silent \
                                continue. Omit to carry on from the end."
            },
            "correction": {
                "type": "string",
                "description": "One instruction appended after the rewind point, saying what to \
                                do differently. Rewinding WITHOUT this usually changes nothing: \
                                turns run at temperature 0, so an identical history reproduces \
                                an identical turn."
            },
            "graceMs": {
                "type": "integer",
                "minimum": 0,
                "description": "How long to block for the turn before returning a responseId to \
                                collect it with. Defaults to 25000; 0 defers immediately."
            },
            "maxTurns": max_turns_property(),
            "resetWorktree": {
                "type": "boolean",
                "description": "With `fromMessageId`: also take the conversation's worktree back \
                                to the commit of the last message the rewind keeps, or to its \
                                base when none kept a commit. Defaults to true; false rewinds the \
                                transcript only and keeps every file. The outcome's \
                                `worktreeReset` says where the worktree now stands and which \
                                commits were dropped."
            },
            (crate::sync_worktree_choice::SYNC_WORKTREE_ARG): crate::sync_worktree_choice::sync_worktree_property(),
            (crate::permission_answer_arg::PERMISSION_ARG): crate::permission_answer_arg::permission_property(),
            (crate::permission_deferral_arg::DEFER_ARG): crate::permission_deferral_arg::defer_property(),
        }
    }))
}

/// The `yieldConditions` property, spelled once for the two tools that accept it: the conditions
/// on a tool call that stop the turn at that call and hand control back to the caller, for this
/// turn only.
fn yield_conditions_property() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["tool", "when"],
            "properties": {
                "tool": {"type": "string", "description": "The tool this condition watches, by \
                            name (READ, STR_REPLACE, …)."},
                "when": {
                    "type": "object",
                    "description": "What to watch for: {\"outcome\": {\"fact\": …}} for a fact of \
                                    the call's result summary, or {\"argument\": {\"field\": …, \
                                    \"contains\": …}} for a string field of the call's arguments \
                                    containing a substring."
                }
            }
        },
        "description": "Stop this turn the moment one of these fires on a tool call, returning \
                        stopReason \"yieldedToCaller\" with the fired condition and the yielded \
                        tool message's id. Bounded: at most 8 conditions, each substring at most \
                        256 chars, each fact one the tool's own result summary can carry. For \
                        this turn only, never the conversation."
    })
}

/// Parse the `yieldConditions` argument of a prompt or resume, or why it cannot run as written.
///
/// Rejected **before** the turn runs, so a malformed condition costs no model turn: the
/// conditions are this caller's own words, and running past a point the caller asked to be
/// handed control at does the opposite of what it asked.
fn yield_conditions_of(
    args: &serde_json::Value,
    request: TurnRequest,
) -> Result<TurnRequest, String> {
    let Some(raw) = args.get("yieldConditions") else {
        return Ok(request);
    };
    let encoded = serde_json::to_string(raw).map_err(|e| format!("yieldConditions: {e}"))?;
    let conditions =
        serde_json::from_str::<Vec<tddy_discovery::subagent::YieldCondition>>(&encoded)
            .map_err(|e| format!("yieldConditions is not a list of conditions: {e}"))?;
    tddy_discovery::subagent::validate_yield_conditions(&conditions)?;
    Ok(request.with_yield_conditions(conditions))
}

/// The `maxTurns` property, spelled once for the two tools that accept it.
///
/// Stated in the description rather than as a schema `maximum`, because the ceiling is not a
/// refusal: a request above it is clamped and the clamp is reported back in the outcome, so a
/// caller that asks for too much keeps working and still learns what it actually got.
fn max_turns_property() -> serde_json::Value {
    serde_json::json!({
        "type": "integer",
        "minimum": SUBAGENT_MIN_TURNS,
        "description": format!(
            "How many model turns this ONE call may spend, in place of the agent definition's \
             own budget. Omit to leave the definition's budget alone. Outside \
             {SUBAGENT_MIN_TURNS}..={SUBAGENT_MAX_TURNS_CEILING} it is cut to the nearer bound, and \
             the outcome then carries `clampedMaxTurns` saying so — a turn that stopped early on a \
             clamped budget has not finished searching."
        )
    })
}

/// The `subagent_await` input schema: the receipt is required, bounding the wait is not.
fn subagent_await_schema() -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    schema_object(serde_json::json!({
        "type": "object",
        "required": ["responseId"],
        "properties": {
            "responseId": {
                "type": "string",
                "description": "The id a subagent_prompt returned instead of an outcome."
            },
            "timeoutMs": {
                "type": "integer",
                "minimum": 0,
                "description": "How long to block before reporting the turn still pending. \
                                Defaults to 25000; 0 reports whatever is ready right now."
            }
        }
    }))
}

/// The `subagent_cancel` tool as the main agent is offered it. What it says it does is what it does:
/// a coding agent's conversation is kept, so the description must not promise a close.
pub(crate) fn subagent_cancel_tool_definition() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "subagent_cancel",
        "Cancel an open subagent conversation's running turn (ACP session/cancel). A coding \
         agent's conversation is kept: its process, context and worktree stay, the reply is \
         {cancelled: true, kept: true}, and it takes the next subagent_prompt. Any other \
         conversation is closed and its worktree deleted.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId"],
            "properties": {
                "sessionId": {"type": "string"}
            }
        })),
    )
}

/// Build the `ToolRouter` for the ACP-shaped subagent conversation tools. Merged into
/// `PermissionServer::new()`'s router only when [`subagent_enabled`].
fn subagent_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<PermissionServer> {
    use rmcp::handler::server::router::tool::ToolRouter;

    let mut router = ToolRouter::new();

    let new_session_tool = rmcp::model::Tool::new(
        "subagent_new_session",
        "Open a new conversation with a discovery subagent (ACP session/new-shaped). \
         Returns {sessionId}.",
        // No agents here: the router is built once, and this copy of the schema is only ever read
        // where the roster is empty — where `advertised_tools` withholds the tool entirely.
        subagent_new_session_schema(&[]),
    );
    router.add_route(subagent_route(new_session_tool, |args| {
        Box::pin(subagent_tools::subagent_new_session_tool(args))
    }));

    let prompt_tool = rmcp::model::Tool::new(
        "subagent_prompt",
        "Send a prompt turn to an open subagent session (ACP session/prompt-shaped). \
         A conversation runs ONE turn at a time: a prompt sent while a turn is in flight is \
         accepted, but it WAITS ITS TURN rather than running alongside it. To get work running in \
         parallel, open a second conversation — more prompts to one conversation is a queue. \
         Blocks for at most `graceMs` (default 25000). A turn that yields in time returns \
         {stopReason, content, usage, messages}. `messages` is what the turn appended, each with \
         an id, a role, `isError` and a short preview: read it to see whether the agent's tool \
         calls actually worked, and to pick an id to hand `subagent_resume` a `fromMessageId`. \
         Pass `maxTurns` to bound this one call. A turn still running when the grace elapses is \
         NOT cancelled: the call returns {responseId, pending: true, queuePosition, queueSize}, \
         the turn keeps running, and `subagent_await` collects its outcome — in the same shape — \
         under that responseId. A responseId is a receipt, not an error. \
         `queuePosition` is how many turns on this conversation were accepted before this one and \
         have not ended yet — 0 means nothing is ahead of it, so it is the turn running now; \
         `queueSize` counts every turn still outstanding on this conversation, this one included. \
         They report what this turn is waiting behind, not a guarantee of the order turns finish \
         in.",
        subagent_prompt_schema(),
    );
    router.add_route(subagent_route(prompt_tool, |args| {
        Box::pin(subagent_tools::subagent_prompt_tool(args))
    }));

    let resume_tool = rmcp::model::Tool::new(
        "subagent_resume",
        "Take another turn on an open subagent conversation WITHOUT sending it a new prompt. \
         Use it when a conversation stopped on its turn budget and you want it to carry on from \
         where it was — re-prompting would make the agent re-read everything it has already read. \
         Pass `fromMessageId` (an id from a turn outcome's `messages`) to send the conversation \
         back to that message first, discarding everything after it; pass `correction` to append \
         one instruction saying what to do differently. A rewind on its own usually changes \
         nothing: turns run at temperature 0, so an identical history reproduces an identical \
         turn — rewind WITH a correction. A conversation runs ONE turn at a time, so a resume is \
         queued exactly like a prompt: it returns {stopReason, content, usage, messages} if it \
         yields within `graceMs` (default 25000), otherwise {responseId, pending: true, \
         queuePosition, queueSize} for `subagent_await` to collect. An unknown sessionId or \
         fromMessageId is an error naming it, never a silent continue.",
        subagent_resume_schema(),
    );
    router.add_route(subagent_route(resume_tool, |args| {
        Box::pin(subagent_tools::subagent_resume_tool(args))
    }));

    let await_tool = rmcp::model::Tool::new(
        "subagent_await",
        "Collect the outcome of a subagent turn that subagent_prompt or subagent_resume handed \
         back a responseId for. Blocks for at most `timeoutMs` (default 25000). Returns the \
         turn's {stopReason, content, usage, messages} once it has ended, or {responseId, \
         pending: true, queuePosition, \
         queueSize} if it is still running — in which case call again with the same responseId. \
         A conversation runs one turn at a time, so a turn with a `queuePosition` above 0 has not \
         started: that many turns accepted before it on the same conversation are still \
         outstanding, and the number falls as they end. `queueSize` is every turn still \
         outstanding on that conversation, this one included. A turn that handed a permission \
         request to the user reports `awaitingUser: true` while the user has not answered it. \
         Collecting an outcome does not consume it: the same responseId can be awaited again.",
        subagent_await_schema(),
    );
    router.add_route(subagent_route(await_tool, |args| {
        Box::pin(subagent_await_tool(args))
    }));

    let cancel_tool = subagent_cancel_tool_definition();
    router.add_route(subagent_route(cancel_tool, |args| {
        Box::pin(subagent_cancel_tool(args))
    }));

    router.add_route(subagent_route(
        crate::subagent_end::subagent_end_tool_definition(),
        |args| Box::pin(crate::subagent_end::subagent_end_tool(args)),
    ));

    router.add_route(subagent_route(
        crate::subagent_pull::subagent_pull_tool_definition(),
        |args| Box::pin(crate::subagent_pull::subagent_pull_tool(args)),
    ));

    router.add_route(subagent_route(
        crate::subagent_diff::subagent_diff_tool_definition(),
        |args| Box::pin(crate::subagent_diff::subagent_diff_tool(args)),
    ));

    let list_tool = rmcp::model::Tool::new(
        "subagent_list",
        "List all open subagent conversations with per-conversation token accounting, how \
         full each one's context is, and how much work is lined up on it. Returns \
         {conversations:[{agent, id, model, inputTokens, outputTokens, totalTokens, turns, \
         contextTokens, queued}]}. `queued` is how many turns are still outstanding on that \
         conversation, the one running included — a conversation runs one turn at a time, so \
         anything above 1 is waiting. `contextTokens` is what that \
         conversation's history costs to send NOW — the last turn's prompt tokens — so it is the \
         figure to watch against the model's window; `inputTokens` is cumulative spend across \
         every turn, which re-sends the history each time and so over-counts the window several \
         times over. A conversation that has ended carries no `contextTokens`: it has no history \
         left to send. Wind a conversation down before `contextTokens` reaches the model's \
         window — a turn refused for a full context ends it, returning a handoff brief to open a \
         new conversation with.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    );
    router.add_route(subagent_route(list_tool, |args| {
        Box::pin(subagent_list_tool(args))
    }));

    let status_tool = rmcp::model::Tool::new(
        "subagent_status",
        "What every agent attached to this session is doing right now, from the session's live \
         roster. Returns {sessionId, appliedRev, agents:[{agentId, label, model, \
         daemonInstanceId, status, cloneState, replaces, conversations, lastActivity?}]}. \
         `status` is one of idle | running | executing_tool | waiting_for_input | connecting | \
         error | unknown; `unknown` means nothing has been observed, NOT that the agent is free. \
         Pass {agent, waitFor:\"ready\"} to park until that agent can be prompted rather than \
         polling; the result then also carries {timedOut}.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "The qualified id of the agent to wait for. Required with \
                                    `waitFor`, ignored without it — a plain read always reports \
                                    every attached agent."
                },
                "waitFor": {
                    "type": "string",
                    "enum": [WAIT_FOR_READY],
                    "description": "Park until `agent` can be prompted: until it is no longer \
                                    `connecting`. An agent whose clone failed settles the wait too \
                                    — waiting will not fix it — and is reported as `error`."
                },
                "timeoutMs": {
                    "type": "integer",
                    "description": "How long to park, default 30000, capped at 120000. Reaching it \
                                    is not an error: the current rows come back with \
                                    `timedOut: true`."
                }
            }
        })),
    );
    router.add_route(subagent_route(status_tool, |args| {
        Box::pin(subagent_status_tool(args))
    }));

    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;
    use rstest::rstest;
    use serial_test::serial;

    #[test]
    fn mcp_server_declares_that_its_tool_list_can_change() {
        // Given a server whose advertised set follows the live roster rather than the router
        let server = PermissionServer::new();

        // When the client reads the tools capability the handshake declares
        let tools = server
            .get_info()
            .capabilities
            .tools
            .expect("the server must declare a tools capability");

        // Then it says the list can change: a client never told that is entitled to ignore
        // `notifications/tools/list_changed`, so an agent attached after the handshake would never
        // reach the catalog the client already read.
        assert_eq!(tools.list_changed, Some(true));
    }

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
            .spawn_conversation(Parameters(SpawnConversationInput(
                tddy_tool_engine::start_session_args::StartSessionArgs {
                    session_type: "workspace".to_string(),
                    ..Default::default()
                },
            )))
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

    /// The PR tool relays the session host's refusal to the agent instead of a generic
    /// authentication failure: the host's words name what to change.
    #[test]
    #[serial]
    fn github_create_pull_request_returns_the_session_hosts_refusal_to_the_agent() {
        // Given a session host whose project assigns no account, reached over TDDY_SOCKET
        let reason = "this project has no github account assigned; assign one in its settings";
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("host.sock");
        serve_github_refusal(&socket, reason);
        std::env::set_var("TDDY_TOOLS_TEST_ALLOW_SOCKET", "1");
        std::env::set_var("TDDY_SOCKET", &socket);
        std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");

        // When the agent calls the create-pull-request tool
        let result = PermissionServer::new().github_create_pull_request(Parameters(
            GithubCreatePullRequestToolInput {
                owner: "acme".to_string(),
                repo: "repo".to_string(),
                title: "t".to_string(),
                head: "h".to_string(),
                base: "b".to_string(),
                body: "b".to_string(),
            },
        ));
        std::env::remove_var("TDDY_TOOLS_TEST_ALLOW_SOCKET");
        std::env::remove_var("TDDY_SOCKET");

        // Then the agent reads the host's reason
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["error"], reason);
    }

    /// Serve a refusal of `github-token` on `socket` from a thread of its own.
    fn serve_github_refusal(socket: &std::path::Path, reason: &'static str) {
        struct Refusing(&'static str);
        #[async_trait::async_trait]
        impl tddy_core::toolcall::GithubCredentialHandler for Refusing {
            async fn github_token(&self) -> Result<String, String> {
                Err(self.0.to_string())
            }
        }
        let bound = socket.to_path_buf();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::UnixListener::bind(&bound).unwrap();
                    ready_tx.send(()).ok();
                    let (tx, _rx) = std::sync::mpsc::sync_channel(1);
                    while let Ok((stream, _)) = listener.accept().await {
                        let service = tddy_core::toolcall::ToolcallRpcService::new(
                            tx.clone(),
                            std::sync::Arc::new(None),
                            std::sync::Arc::new(None),
                            std::sync::Arc::new(std::env::temp_dir()),
                        )
                        .with_github_credential_handler(Some(std::sync::Arc::new(Refusing(
                            reason,
                        ))));
                        let (reader, writer) = stream.into_split();
                        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                            reader,
                            writer,
                            service,
                            tddy_rpc::RequestTransport::UnixSocket,
                        );
                        tokio::spawn(endpoint.run());
                    }
                });
        });
        ready_rx.recv().unwrap();
    }

    /// `call_tool_by_name` (the web Inspector invoke path) dispatches a side-effect-free MCP tool
    /// in-process and returns its result; an unknown name is a clean error, not a panic.
    #[tokio::test]
    async fn call_tool_by_name_dispatches_mcp_tool_and_rejects_unknown() {
        let server = PermissionServer::new();

        // pr_stack_status is forwarded to the daemon; with no daemon configured the session-tool
        // client answers with its own error JSON (a real result string), proving the dispatch
        // reached the forwarding path.
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

    /// The relayed request carries `StartSession`'s arguments as given, under the
    /// `start-conversation` verb, so the daemon starts exactly what the agent asked for.
    #[test]
    fn start_conversation_request_relays_the_start_session_arguments() {
        // Given a workspace on a named branch
        let args = tddy_tool_engine::start_session_args::StartSessionArgs {
            session_type: "workspace".to_string(),
            new_branch_name: "implement-foo".to_string(),
            ..Default::default()
        };

        // When the request is built
        let request = start_conversation_request_json(&args);

        // Then the relayed request carries the verb and the arguments
        assert_eq!(request["type"], "start-conversation");
        assert_eq!(request["args"]["session_type"], "workspace");
        assert_eq!(request["args"]["new_branch_name"], "implement-foo");
    }

    // ─── Deferring a turn that outruns its caller's patience ──────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 25-29, 31, 34)

    use std::time::Duration;

    // ─── The blocking budget a caller names ───────────────────────────────────

    /// How long a call may block is the caller's fact — a workflow step waiting on a coding agent
    /// and a discovery question mid-edit want different numbers from the same process.
    #[rstest]
    #[case::grace("graceMs")]
    #[case::timeout("timeoutMs")]
    fn a_blocking_budget_the_caller_names_is_the_one_used(#[case] field: &str) {
        // Given a call naming its own budget
        let args = serde_json::json!({ field: 750 });

        // When the tool reads it
        let budget =
            subagent_tools::blocking_budget(&args, field, subagent_tools::SUBAGENT_PROMPT_GRACE);

        // Then the caller's number, in milliseconds, is what it blocks for
        assert_eq!(budget, Ok(Duration::from_millis(750)));
    }

    /// The default exists so the parameter can be omitted, not so a malformed one can be ignored.
    #[test]
    fn a_call_that_names_no_blocking_budget_blocks_for_the_default() {
        // Given a call that names none
        let args = serde_json::json!({ "sessionId": "conv-1" });

        // When the tool reads it
        let budget = subagent_tools::blocking_budget(
            &args,
            "graceMs",
            subagent_tools::SUBAGENT_PROMPT_GRACE,
        );

        // Then it blocks for the documented default
        assert_eq!(budget, Ok(SUBAGENT_PROMPT_GRACE));
    }

    /// Zero is a budget, not an absence: "hand me the id, I will collect it myself" is exactly what
    /// an agent dispatching parallel work wants, and reading it as "unset" would block it for 25s.
    #[test]
    fn a_zero_blocking_budget_defers_the_turn_without_blocking() {
        // Given a call that will not wait at all
        let args = serde_json::json!({ "graceMs": 0 });

        // When the tool reads it
        let budget = subagent_tools::blocking_budget(
            &args,
            "graceMs",
            subagent_tools::SUBAGENT_PROMPT_GRACE,
        );

        // Then it blocks for no time, rather than falling back to the default
        assert_eq!(budget, Ok(Duration::ZERO));
    }

    /// Substituting the default for a budget the caller got wrong would silently block a call for
    /// 25 seconds that asked for something else entirely, with nothing saying so.
    #[rstest]
    #[case::text(serde_json::json!({"graceMs": "soon"}))]
    #[case::negative(serde_json::json!({"graceMs": -1}))]
    fn a_blocking_budget_that_is_not_a_count_of_milliseconds_is_refused(
        #[case] args: serde_json::Value,
    ) {
        // When the tool reads it
        let budget = subagent_tools::blocking_budget(
            &args,
            "graceMs",
            subagent_tools::SUBAGENT_PROMPT_GRACE,
        );

        // Then it is refused naming the field, not quietly replaced by the default
        let refusal = budget.expect_err("a malformed budget must be refused, not defaulted");
        assert!(
            refusal.contains("graceMs"),
            "the refusal must name the field the caller got wrong; got: {refusal:?}"
        );
    }

    // ─── The catalog ──────────────────────────────────────────────────────────

    /// The advertisement filter reads off the router, so a tool absent from the router is a tool no
    /// agent can ever call — and an agent handed a `responseId` with no way to redeem it.
    #[test]
    fn the_subagent_router_offers_a_tool_for_collecting_a_deferred_turn() {
        // When the conversation tools are built
        let names = subagent_tools::subagent_tool_names();

        // Then the one that collects a deferred turn is among them
        assert!(
            names.contains(&"subagent_await".to_string()),
            "the subagent router must offer 'subagent_await'; got: {names:?}"
        );
    }

    /// An agent cannot pass a parameter the schema does not name, so `graceMs` being declared is
    /// what makes the blocking budget the caller's choice rather than the host's.
    #[test]
    fn the_prompt_tool_schema_names_the_callers_blocking_budget() {
        // Given the advertised prompt tool
        let router = subagent_tool_router();
        let prompt_tool = router
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "subagent_prompt")
            .expect("the subagent router must offer 'subagent_prompt'");

        // Then its input schema declares the budget, and does not require it
        let properties = prompt_tool.input_schema["properties"]
            .as_object()
            .expect("subagent_prompt must declare its properties");
        assert!(
            properties.contains_key("graceMs"),
            "subagent_prompt must accept 'graceMs'; got: {:?}",
            properties.keys().collect::<Vec<_>>()
        );
        let required: Vec<&str> = prompt_tool.input_schema["required"]
            .as_array()
            .expect("subagent_prompt must declare its required fields")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            !required.contains(&"graceMs"),
            "a call that names no budget must still be valid; got required: {required:?}"
        );
    }

    /// The await tool is useless without the id it collects, and pointless if the caller cannot
    /// bound how long it waits — the two together are its whole input.
    #[test]
    fn the_await_tool_schema_requires_the_response_id_and_bounds_the_wait() {
        // Given the advertised await tool
        let router = subagent_tool_router();
        let await_tool = router
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "subagent_await")
            .expect("the subagent router must offer 'subagent_await'");

        // Then it requires the id, and accepts an optional bound on the wait
        let required: Vec<&str> = await_tool.input_schema["required"]
            .as_array()
            .expect("subagent_await must declare its required fields")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(
            required,
            vec!["responseId"],
            "subagent_await must require the response id and nothing else"
        );
        let properties = await_tool.input_schema["properties"]
            .as_object()
            .expect("subagent_await must declare its properties");
        assert!(
            properties.contains_key("timeoutMs"),
            "subagent_await must accept 'timeoutMs'; got: {:?}",
            properties.keys().collect::<Vec<_>>()
        );
    }

    // ─── Opening a conversation under the caller's own system prompt ──────────

    /// The roster snapshot these tests publish. The registry is process-wide and only moves
    /// forward, so a literal revision would make them depend on the order they run in.
    fn next_roster_rev() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_REV: AtomicU64 = AtomicU64::new(1_000);
        NEXT_REV.fetch_add(1, Ordering::SeqCst)
    }

    /// The variables a remote route to the daemon is configured through. A test run inside a
    /// tddy session inherits them, and the transport then exists where the test expects none.
    const REMOTE_ROUTE_VARS: [&str; 4] = [
        "TDDY_REMOTE_SESSION_ID",
        "TDDY_REMOTE_SESSION_TOKEN",
        "TDDY_REMOTE_DAEMON_URL",
        "TDDY_REMOTE_DAEMON_SOCKET",
    ];

    /// Clears [`REMOTE_ROUTE_VARS`] until dropped, then restores what each held. Callers are
    /// `#[serial]`, so no other test reads the environment meanwhile.
    struct WithoutARemoteRoute(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl WithoutARemoteRoute {
        fn new() -> Self {
            let saved = REMOTE_ROUTE_VARS
                .iter()
                .map(|&name| (name, std::env::var_os(name)))
                .collect();
            for name in REMOTE_ROUTE_VARS {
                std::env::remove_var(name);
            }
            Self(saved)
        }
    }

    impl Drop for WithoutARemoteRoute {
        fn drop(&mut self) {
            for (name, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    /// An agent whose turn loop runs somewhere this process holds no def for — which is every
    /// roster agent in a split session, where `tddy-tools` lives in the jail and the defs do not.
    fn a_session_with_one_remotely_routed_agent(agent_id: &str) {
        let (name, daemon) = agent_id
            .split_once('@')
            .expect("a roster agent id is qualified");
        crate::session_agents::session_agent_roster().apply_snapshot(
            tddy_service::proto::session_agents_svc::SessionAgentRoster {
                session_id: "1790497976955-remote-roster".to_string(),
                rev: next_roster_rev(),
                agents: vec![tddy_service::proto::session_agents_svc::SessionAgentEntry {
                    agent_id: agent_id.to_string(),
                    name: name.to_string(),
                    daemon_instance_id: daemon.to_string(),
                    label: format!("{name} (on {daemon})"),
                    model: "gemma-3-27b:latest".to_string(),
                    replaces: Vec::new(),
                    tools: vec!["Read".to_string(), "Grep".to_string()],
                    codebase_session_id: "ws-01-clone".to_string(),
                    clone_state: 3, // AGENT_CLONE_STATE_READY
                    clone_error: String::new(),
                    status: 0, // SESSION_AGENT_STATUS_UNSPECIFIED
                    last_activity: None,
                    kind: 0,
                    harness: String::new(),
                    placement: 0,
                    transport: String::new(),
                    permission_policy: String::new(),
                }],
            },
        );
    }

    /// The `error` an MCP subagent tool reports inside its success envelope.
    fn the_refusal_in(result: &str) -> String {
        let body: serde_json::Value = serde_json::from_str(result)
            .unwrap_or_else(|e| panic!("a subagent tool returns JSON, got {result:?}: {e}"));
        body["error"]
            .as_str()
            .unwrap_or_else(|| panic!("the call should have been refused, got: {body}"))
            .to_string()
    }

    /// The whole point of the override: in a split session every roster agent is run by the
    /// facilitating daemon, so a `systemPrompt` refused for being remotely routed is a
    /// `systemPrompt` that does not exist for the configuration most sessions run in.
    ///
    /// The open cannot succeed here — there is no daemon to reach — so what is pinned is *which*
    /// refusal comes back: the missing transport, not the override.
    #[tokio::test]
    #[serial]
    async fn a_system_prompt_for_a_remotely_routed_agent_is_carried_rather_than_refused() {
        // Given a session whose only agent is run by another daemon
        let _route = WithoutARemoteRoute::new();
        a_session_with_one_remotely_routed_agent("Gemma@ws-01");

        // When a conversation with it is opened under a prompt of the caller's own
        let result = subagent_tools::subagent_new_session_tool(serde_json::json!({
            "agent": "Gemma@ws-01",
            "sessionId": "conv-override-remote",
            "systemPrompt": "Answer only with file paths, one per line.",
        }))
        .await;

        // Then the open was attempted and failed for want of a daemon, not refused for the prompt
        assert!(
            the_refusal_in(&result).contains(tddy_discovery::roster::NO_TRANSPORT),
            "the override must reach the daemon that runs the agent; got: {result}"
        );
    }

    /// A blank override is a caller that meant to send something, and it is refused the same way
    /// on both paths — an empty `system_prompt` on the wire *means* "use the def's own", so
    /// sending one would open the conversation under a prompt nobody chose.
    #[tokio::test]
    #[serial]
    async fn a_blank_system_prompt_for_a_remotely_routed_agent_is_refused_naming_it() {
        // Given a session whose only agent is run by another daemon
        let _route = WithoutARemoteRoute::new();
        a_session_with_one_remotely_routed_agent("Gemma@ws-02");

        // When a conversation with it is opened under a prompt of nothing but spaces
        let result = subagent_tools::subagent_new_session_tool(serde_json::json!({
            "agent": "Gemma@ws-02",
            "sessionId": "conv-blank-remote",
            "systemPrompt": "   ",
        }))
        .await;

        // Then the caller is told the override is blank
        assert!(
            the_refusal_in(&result).contains("blank"),
            "a blank override must be named rather than dropped; got: {result}"
        );
    }
}
