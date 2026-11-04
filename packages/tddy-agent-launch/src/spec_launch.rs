//! The spec-driven host PTY launcher: a coding agent declared as a tool
//! (`tddy-coding-agent-specs`) is resolved on the host, its argv rendered from its `run:` template,
//! its slots written, and it is spawned in a PTY — with nothing agent-specific in this path.
//!
//! Spec: `docs/ft/daemon/claude-cli-session.md` § Launch (`#agent-tools 2/12`). The
//! unsandboxed `claude-cli` start and resume are its first callers; the jail (`#agent-tools 5/12`)
//! reuses the rendering, not this launcher.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::{
    render_argv, ResolvedTool, TemplateParams, ToolDef, ToolError, ToolHost, ToolRegistry,
    Transport,
};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_coding_agent_specs::slots::{
    self, McpServerCommand, RenderedSlots, SlotError, SlotInputs,
};
use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec, SpecError};
use tddy_daemon_kernel::config::DaemonConfig;

/// The coding agents this host can launch: the agent registry
/// (`tddy_coding_agent_specs::load_agent_registry`) and the host its tools resolve against.
///
/// Installed on the session host by the daemon runtime; a host without it launches `claude-cli`
/// the hand-built way. `TODO(agent-tools 12/12)`: the hand-built way goes once the session-type
/// gates move onto specs (`docs/dev/todo/2026-11-02-claude-cli-hand-built-launch-removal.md`).
pub struct HostCodingAgents {
    registry: ToolRegistry,
    agents: Arc<CodingAgentRegistry>,
    host: Arc<dyn ToolHost>,
}

impl HostCodingAgents {
    /// The agents of `registry` (a [`tddy_coding_agent_specs::load_agent_registry`] result) on `host`.
    ///
    /// # Panics
    /// When a tool of `registry` has a `coding_agent` extension and no `run:` section — a host
    /// start reads its registry through [`Self::try_new`] and refuses instead.
    pub fn new(registry: ToolRegistry, host: Arc<dyn ToolHost>) -> Self {
        Self::try_new(registry, host).expect("a registry holding only usable coding agents")
    }

    /// [`Self::new`], refused when a tool with a `coding_agent` extension is not a usable agent.
    pub fn try_new(
        registry: ToolRegistry,
        host: Arc<dyn ToolHost>,
    ) -> Result<Self, tddy_coding_agent_specs::RegistryError> {
        let agents = Arc::new(CodingAgentRegistry::from_tools(registry.clone())?);
        Ok(Self {
            registry,
            agents,
            host,
        })
    }

    /// The coding agents among the tools: what a gate asks for an agent's capabilities.
    pub fn agents(&self) -> &CodingAgentRegistry {
        &self.agents
    }

    /// [`Self::agents`], shared with a service that gates on it.
    pub fn agents_shared(&self) -> Arc<CodingAgentRegistry> {
        Arc::clone(&self.agents)
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// The host the agents' tools resolve against, and whose environment a jailed tool's
    /// `host-env` secrets are read from.
    pub fn host(&self) -> &dyn ToolHost {
        self.host.as_ref()
    }

    /// Agent `id` resolved on this host: its spec, its own tool and the closure of its deps.
    /// `binary_override` — daemon.yaml `claude_cli.binary_path` until `#agent-tools 12/12` — wins
    /// over the spec's binary resolution when set.
    pub fn resolve(
        &self,
        id: &str,
        binary_override: Option<&Path>,
    ) -> Result<HostAgent, SpecLaunchError> {
        let def = self
            .registry
            .get(id)
            .ok_or_else(|| SpecLaunchError::UnknownAgent(id.to_string()))?;
        let spec = CodingAgentSpec::of(def).map_err(SpecLaunchError::Spec)?;
        let (tool, deps) = match binary_override {
            None => {
                let mut closure = self
                    .registry
                    .resolve_closure(&[id], self.host.as_ref())
                    .map_err(SpecLaunchError::Tool)?;
                // The closure lists deps first, so the agent's own tool is last.
                let tool = closure
                    .pop()
                    .ok_or_else(|| SpecLaunchError::UnknownAgent(id.to_string()))?;
                (tool, closure)
            }
            Some(binary) => {
                let dep_ids: Vec<&str> = def.deps.iter().map(String::as_str).collect();
                let deps = self
                    .registry
                    .resolve_closure(&dep_ids, self.host.as_ref())
                    .map_err(SpecLaunchError::Tool)?;
                (self.overridden_tool(def, binary)?, deps)
            }
        };
        Ok(HostAgent { spec, tool, deps })
    }

    /// `def` as the tool at `binary`, which must be an executable file on this host.
    fn overridden_tool(
        &self,
        def: &ToolDef,
        binary: &Path,
    ) -> Result<ResolvedTool, SpecLaunchError> {
        let unresolvable = |reason: String| {
            SpecLaunchError::Tool(ToolError::Unresolvable {
                tool: def.id.clone(),
                searched: format!("{} ({reason})", binary.display()),
            })
        };
        if !self.host.is_executable_file(binary) {
            return Err(unresolvable("not an executable file".to_string()));
        }
        let canonical = self
            .host
            .canonicalize(binary)
            .map_err(|e| unresolvable(format!("cannot be resolved: {e}")))?;
        Ok(ResolvedTool {
            id: def.id.clone(),
            command: def.binary.command.clone(),
            binary: canonical,
            reads: def.reads.clone(),
            network: def.network.clone(),
        })
    }
}

/// One coding agent as this host satisfies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAgent {
    pub spec: CodingAgentSpec,
    /// The agent's own tool (`{binary}` in its template).
    pub tool: ResolvedTool,
    /// Its deps, resolved (`tddy-tools`), deps first.
    pub deps: Vec<ResolvedTool>,
}

impl HostAgent {
    /// The resolved dep `id` (`tddy-tools`, the relay its MCP slot and hooks name).
    pub fn dep(&self, id: &str) -> Option<&ResolvedTool> {
        self.deps.iter().find(|tool| tool.id == id)
    }
}

/// Where a host PTY launch is registered and as whom it runs.
pub struct HostPtySession<'a> {
    pub manager: &'a Arc<CliSessionManager>,
    pub session_id: &'a str,
    /// The session type the terminal is registered under (`claude-cli`).
    pub session_type: &'a str,
    pub model: &'a str,
    /// Set on the process on top of the daemon's and the spec's `run.env`.
    pub env: Vec<(String, String)>,
    /// The session owner; `None` runs the agent as the daemon.
    pub os_user: Option<&'a str>,
}

/// Why a spec-driven launch did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecLaunchError {
    /// The registry has no tool of that id.
    UnknownAgent(String),
    Tool(ToolError),
    Spec(SpecError),
    /// The launcher runs PTY agents only; a spec declaring another transport is refused.
    TransportNotPty {
        spec: String,
        transport: Transport,
    },
    /// A slot could not be rendered.
    Slots(SlotError),
    /// A slot file could not be written.
    SlotFile {
        path: PathBuf,
        message: String,
    },
    /// The PTY spawn failed.
    Spawn(String),
}

impl fmt::Display for SpecLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAgent(id) => write!(f, "unknown coding agent {id}"),
            Self::Tool(e) => write!(f, "{e}"),
            Self::Spec(e) => write!(f, "{e}"),
            Self::Slots(e) => write!(f, "{e}"),
            Self::TransportNotPty { spec, transport } => write!(
                f,
                "coding agent {spec} declares transport {transport:?}; the host PTY launcher runs pty agents only"
            ),
            Self::SlotFile { path, message } => {
                write!(f, "slot file {}: {message}", path.display())
            }
            Self::Spawn(message) => write!(f, "failed to spawn the agent: {message}"),
        }
    }
}

impl std::error::Error for SpecLaunchError {}

/// What a spec renders for one host launch: the argv and the spec's own `run.env`, the slot files
/// already written. A launcher that does not spawn through [`launch_host_pty`] — a tmux-run agent
/// (`#tmux-agent`) — starts this argv itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPtyPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Plan `agent`'s host launch: refuse a spec whose `run.transport` is not `pty`, write
/// `slots.files`, and render `run.argv` with `params` plus `slots.argv_extra` bound to the `slots`
/// list.
pub fn plan_host_pty(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
) -> Result<HostPtyPlan, SpecLaunchError> {
    if agent.spec.run.transport != Transport::Pty {
        return Err(SpecLaunchError::TransportNotPty {
            spec: agent.spec.id.clone(),
            transport: agent.spec.run.transport,
        });
    }
    for (path, bytes) in &slots.files {
        write_slot_file(path, bytes)?;
    }
    let params = params.clone().with_list("slots", slots.argv_extra.clone());
    let argv = render_argv(&agent.spec.run, &agent.tool, &params).map_err(SpecLaunchError::Tool)?;
    let env = agent
        .spec
        .run
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    Ok(HostPtyPlan { argv, env })
}

/// Spawn `agent` in a PTY in `worktree` from its [`plan_host_pty`], and register the process under
/// `session`. `on_exit` runs once the process has exited, with its terminal's output (a resume
/// token recorder, `#agent-tools 8/12`); `None` runs nothing.
pub async fn launch_host_pty(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    worktree: &Path,
    session: HostPtySession<'_>,
    on_exit: Option<ExitHook>,
) -> Result<Arc<PtyHandle>, SpecLaunchError> {
    let HostPtyPlan { argv, env } = plan_host_pty(agent, params, slots)?;
    let env = env.into_iter().chain(session.env).collect();
    session
        .manager
        .spawn_rendered_main(
            session.session_id,
            session.session_type,
            worktree.to_path_buf(),
            session.model,
            argv,
            env,
            session.os_user,
            on_exit,
        )
        .await
        .map_err(|e| SpecLaunchError::Spawn(e.to_string()))
}

fn write_slot_file(path: &Path, bytes: &[u8]) -> Result<(), SpecLaunchError> {
    let slot_file = |message: String| SpecLaunchError::SlotFile {
        path: path.to_path_buf(),
        message,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| slot_file(e.to_string()))?;
    }
    tddy_core::atomic_file::write_atomic(path, bytes).map_err(|e| slot_file(e.to_string()))
}

/// A `claude-cli` start or resume, as the launch steps before the spawn have settled it.
pub struct ClaudeCliLaunch<'a> {
    pub agents: &'a HostCodingAgents,
    /// Where daemon.yaml `claude_cli.binary_path` is read from.
    pub config: &'a DaemonConfig,
    pub manager: &'a Arc<CliSessionManager>,
    pub session_id: &'a str,
    pub worktree: &'a Path,
    pub model: &'a str,
    /// Continue the existing transcript (`--resume`) rather than assign the id (`--session-id`).
    pub resume: bool,
    pub initial_prompt: Option<&'a str>,
    pub permission_mode: Option<&'a str>,
    pub dangerously_skip_permissions: bool,
    pub append_system_prompt_file: Option<&'a Path>,
    /// The tool server registered and the tools pre-approved and withdrawn, through the slots.
    pub toolset: AgentToolset,
    /// The session the `hooks` slot writes its settings for; `None` leaves the hooks file alone (a
    /// resume, which keeps the file its start wrote).
    pub hooks: Option<tddy_core::HookCommandParams<'a>>,
    /// Flags already rendered by the caller (a split session's wiring), placed after the slot flags
    /// and before the prompt.
    pub extra_args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub os_user: Option<&'a str>,
}

/// What a launch hands its agent to call: the tool server it registers, the tools it pre-approves
/// and the tools it withdraws. Rendered by the agent's `mcp`, `tool_allow` and `tool_deny` slots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentToolset {
    /// `None` registers nothing and adds no flag.
    pub mcp: Option<McpRegistration>,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
}

/// An MCP server and the directory its registration file is written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRegistration {
    pub server: McpServerCommand,
    pub dir: PathBuf,
}

/// The agent every `claude-cli` session runs.
pub(crate) const CLAUDE_AGENT_ID: &str = "claude";

/// Launch a `claude-cli` session from the `claude` spec: resolve it, render its slots and its argv,
/// and spawn it with [`launch_host_pty`].
pub async fn launch_claude_cli(
    launch: ClaudeCliLaunch<'_>,
) -> Result<Arc<PtyHandle>, SpecLaunchError> {
    let agent = launch
        .agents
        .resolve(CLAUDE_AGENT_ID, configured_binary(launch.config).as_deref())?;
    let params = claude_template_params(&launch);
    let AgentToolset {
        mcp,
        allowed_tools,
        denied_tools,
    } = launch.toolset;
    let (mcp_server, scratch_dir) = match mcp {
        Some(McpRegistration { server, dir }) => (Some(server), dir),
        None => (None, launch.worktree.to_path_buf()),
    };
    let mut slots = slots::render(
        &agent.spec,
        &SlotInputs {
            workdir: launch.worktree.to_path_buf(),
            scratch_dir,
            mcp_server,
            allowed_tools,
            denied_tools,
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: launch.append_system_prompt_file.map(Path::to_path_buf),
            hooks: launch.hooks,
            home: launch.agents.host().env_var("HOME").map(PathBuf::from),
        },
    )
    .map_err(SpecLaunchError::Slots)?;
    slots.argv_extra.extend(launch.extra_args);

    launch_host_pty(
        &agent,
        &params,
        &slots,
        launch.worktree,
        HostPtySession {
            manager: launch.manager,
            session_id: launch.session_id,
            session_type: "claude-cli",
            model: launch.model,
            env: launch.env,
            os_user: launch.os_user,
        },
        None,
    )
    .await
}

/// The `claude` template's parameters for `launch`. The permission mode defaults to `auto`, and a
/// blank prompt is no prompt — both as the hand-built argv had them.
fn claude_template_params(launch: &ClaudeCliLaunch<'_>) -> TemplateParams {
    TemplateParams::new()
        .with("model", launch.model)
        .with("session_id", launch.session_id)
        .with_flag("resume", launch.resume)
        .with_flag(
            "dangerously_skip_permissions",
            launch.dangerously_skip_permissions,
        )
        .with(
            "permission_mode",
            non_blank(launch.permission_mode).unwrap_or("auto"),
        )
        .with_opt("prompt", non_blank(launch.initial_prompt))
}

/// `value` trimmed, or `None` when nothing but whitespace is left.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// daemon.yaml `claude_cli.binary_path` when it names a path; a bare name leaves the binary to the
/// spec's own resolution.
pub(crate) fn configured_binary(config: &DaemonConfig) -> Option<PathBuf> {
    config
        .claude_cli
        .as_ref()
        .map(|claude_cli| claude_cli.binary_path.as_str())
        .filter(|path| path.contains('/'))
        .map(PathBuf::from)
}
