//! What a sandboxed `claude-cli` start and its relaunch hand the shared jail launcher: the `claude`
//! tool definition resolved on this host, its slots rendered for jail paths, and the session's
//! launch inputs (`#agent-tools 5/12`).
//!
//! The two callers differ only in `resume` and in where their inputs come from; the argv, the MCP
//! registration, the tool allow- and deny-lists and the launcher call are the same, so they are
//! built once, here.

use std::collections::BTreeMap;
use std::path::Path;

use tddy_actions::tool::{TemplateParams, ToolDef};
use tddy_coding_agent_specs::claude_tools::{
    build_claude_allowlist, build_claude_disallowlist, effective_replaced_tools,
    PERMISSION_PROMPT_TOOL,
};
use tddy_coding_agent_specs::slots::{self, McpServerCommand, SlotInputs};
use tddy_coding_agent_specs::{CodingAgentSpec, McpSlot};
use tddy_core::provider_route::HarnessRoute;
use tddy_rpc::Status;
use tddy_session_agents::roster_replacement::roster_replacement_pairs;

use crate::jail_launch::{jail_home, mcp_server_env, JailSpec, JailedSession};
use crate::launch_ports::LaunchSessions;
use crate::spec_launch::{configured_binary, HostAgent, CLAUDE_AGENT_ID};
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// The dep of the `claude` and `cursor` tools that serves their MCP tools.
pub(crate) const TDDY_TOOLS_ID: &str = "tddy-tools";

/// A sandboxed `claude-cli` launch, as its start or relaunch has settled it.
pub(crate) struct ClaudeJail<'a> {
    pub(crate) jail: &'a JailSession<'a>,
    pub(crate) dirs: &'a JailDirs,
    pub(crate) model: &'a str,
    /// Continue the existing transcript (`--resume`) rather than assign the id (`--session-id`).
    pub(crate) resume: bool,
    /// Blank is `auto`.
    pub(crate) permission_mode: &'a str,
    pub(crate) dangerously_skip_permissions: bool,
    /// Client-supplied pass-through args and the first prompt, passed to `claude` ahead of the slot
    /// flags: the MCP flag is variadic and would swallow a bare positional placed after it.
    pub(crate) passthrough_args: Vec<String>,
    pub(crate) append_system_prompt_file: Option<&'a Path>,
    /// The session's roster, which decides the tools the main agent has given away.
    pub(crate) agents: &'a [tddy_core::SessionAgentRecord],
    /// What the daemon adds for the session: see [`JailedSession::env`].
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) session_env: Vec<(String, String)>,
    pub(crate) managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    pub(crate) managed_codebase: bool,
    pub(crate) route: Option<HarnessRoute>,
    /// The session's granted role; empty for none.
    pub(crate) role: &'a str,
}

impl LaunchSessions {
    /// Launch `claude` in a jail for `launch` and return the runner's pid.
    // TODO(cli-dispatch-collapse): deleted at green — § Removed of docs/dev/1-WIP/2026-11-04-agent-tools-cli-dispatch-collapse.md.
    pub(crate) async fn launch_jailed_claude(&self, launch: ClaudeJail<'_>) -> Result<u32, Status> {
        let agents = self.coding_agents.as_deref().ok_or_else(|| {
            Status::failed_precondition(
                "this host has no coding agents installed: a sandboxed claude-cli session is \
                 launched from the `claude` tool definition",
            )
        })?;
        let agent = agents
            .resolve(CLAUDE_AGENT_ID, configured_binary(&self.config).as_deref())
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        let defs = closure_defs(agents.registry(), &agent)?;
        let tddy_tools = agent
            .dep(TDDY_TOOLS_ID)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "the {CLAUDE_AGENT_ID} tool does not depend on {TDDY_TOOLS_ID}, which a \
                     jailed agent reaches its tools through"
                ))
            })?
            .binary
            .clone();

        let (subagent_enabled, replaced) = withdrawn_tools(launch.agents, &launch.env);
        let replaced: Vec<&str> = replaced.iter().map(String::as_str).collect();
        let server = McpServerCommand {
            name: TDDY_TOOLS_ID.to_string(),
            command: tddy_tools,
            args: vec!["--mcp".to_string()],
            env: mcp_server_env(launch.jail.session_id, launch.dirs, &launch.env),
        };
        let tool_def = defs
            .iter()
            .find(|def| def.id == agent.tool.id)
            .ok_or_else(|| {
                Status::internal(format!(
                    "the registry holds no definition of {}",
                    agent.tool.id
                ))
            })?;
        let home = jail_home(
            tool_def,
            &self.tddy_data_dir,
            &launch.dirs.scratch_dir.join("home"),
        )?;
        let mut rendered = slots::render(
            &jailed(&agent.spec),
            &SlotInputs {
                workdir: launch.dirs.context_dir.clone(),
                scratch_dir: launch.dirs.scratch_tmp.clone(),
                mcp_server: Some(server),
                allowed_tools: build_claude_allowlist(subagent_enabled, &replaced),
                denied_tools: build_claude_disallowlist(&replaced),
                permission_prompt_tool: Some(PERMISSION_PROMPT_TOOL.to_string()),
                preamble: None,
                append_system_prompt_file: launch.append_system_prompt_file.map(Path::to_path_buf),
                hooks: None,
                home: Some(home),
            },
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        rendered.argv_extra.splice(0..0, launch.passthrough_args);

        let permission_mode = Some(launch.permission_mode.trim())
            .filter(|mode| !mode.is_empty())
            .unwrap_or("auto");
        let params = TemplateParams::new()
            .with("model", launch.model)
            .with("session_id", launch.jail.session_id)
            .with_flag("resume", launch.resume)
            .with_flag(
                "dangerously_skip_permissions",
                launch.dangerously_skip_permissions,
            )
            .with("permission_mode", permission_mode);
        self.launch_jailed(
            JailSpec {
                agent: &agent,
                defs: &defs,
                host: agents.host(),
            },
            &params,
            &rendered,
            JailedSession {
                jail: launch.jail,
                dirs: launch.dirs,
                session_env: launch.session_env,
                env: launch.env,
                managed: launch.managed,
                managed_codebase: launch.managed_codebase,
                route: launch.route,
            },
            launch.role,
        )
        .await
    }
}

/// `spec` as a jailed agent loads its MCP servers: the jail's context directory holds the
/// project's own `.mcp.json`, which the agent has always loaded beside `tddy-tools`, so the slot's
/// strict flag — which would drop it — is not passed.
// TODO(claude-jail): decide whether a jailed agent should load only `tddy-tools`, as an
// unsandboxed main does; today's jail did not restrict it
// (docs/dev/todo/2026-11-02-jailed-claude-loads-project-mcp-json.md).
fn jailed(spec: &CodingAgentSpec) -> CodingAgentSpec {
    let mut spec = spec.clone();
    if let McpSlot::File { strict_flag, .. } = &mut spec.agent.mcp {
        *strict_flag = None;
    }
    spec
}

/// The definition of `agent`'s own tool and of each of its deps.
pub(crate) fn closure_defs(
    registry: &tddy_actions::tool::ToolRegistry,
    agent: &HostAgent,
) -> Result<Vec<ToolDef>, Status> {
    agent
        .deps
        .iter()
        .chain([&agent.tool])
        .map(|tool| {
            registry.get(&tool.id).cloned().ok_or_else(|| {
                Status::internal(format!("the registry holds no definition of {}", tool.id))
            })
        })
        .collect()
}

/// Whether the session has subagents, and the exec tools its roster has given away — what the
/// main agent's tool allow- and deny-lists are computed from. `SemanticSearch` joins the withdrawn
/// tools unless the session was started with a semantic index, which the daemon signals through
/// `TDDY_SEMANTIC_INDEX_DB`.
fn withdrawn_tools(
    agents: &[tddy_core::SessionAgentRecord],
    env: &BTreeMap<String, String>,
) -> (bool, Vec<String>) {
    let set = |name: &str| env.get(name).is_some_and(|value| !value.trim().is_empty());
    let mut given_away: Vec<String> = roster_replacement_pairs(agents)
        .into_iter()
        .flat_map(|(_, tools)| tools)
        .collect();
    given_away.sort();
    given_away.dedup();
    let given_away_refs: Vec<&str> = given_away.iter().map(String::as_str).collect();
    let replaced = effective_replaced_tools(&given_away_refs, set("TDDY_SEMANTIC_INDEX_DB"))
        .into_iter()
        .map(str::to_string)
        .collect();
    (set("TDDY_SUBAGENT"), replaced)
}
