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
use tddy_coding_agent_specs::slots::{McpServerCommand, RenderedSlots, SlotError};
use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec, SpecError};

/// The coding agents this host can launch: the agent registry
/// (`tddy_coding_agent_specs::load_agent_registry`) and the host its tools resolve against.
///
/// Installed on the session host by the daemon runtime; a host without it refuses every CLI agent's
/// start, resume and child with `FAILED_PRECONDITION`, and re-attaches nothing.
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
        Ok(HostAgent {
            spec,
            tool,
            deps,
            env_base: self.agents.agent_env_base().clone(),
        })
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
    /// The base set the agent's environment keeps from the daemon's: its host registry's
    /// (`#agent-tools 28/28`).
    pub env_base: tddy_coding_agent_specs::AgentEnvBase,
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
    /// A slot file on an SSH target was not written in time: the target never answered, and the
    /// ssh was killed.
    SlotFileOverSshTimedOut {
        host: String,
        path: PathBuf,
        after: std::time::Duration,
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
            Self::SlotFileOverSshTimedOut { host, path, after } => write!(
                f,
                "slot file {} on ssh target {host}: no answer within {after:?}; the ssh was killed",
                path.display()
            ),
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
    spawn_host_pty(agent, params, slots, worktree, session, on_exit, false).await
}

/// [`launch_host_pty`] for the generic CLI launch: the agent runs without the `TDDY_*` variables
/// the daemon's own environment holds (stack rule) — it sees only what `session.env` names.
pub(crate) async fn launch_host_pty_without_inherited_tddy(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    worktree: &Path,
    session: HostPtySession<'_>,
    on_exit: Option<ExitHook>,
) -> Result<Arc<PtyHandle>, SpecLaunchError> {
    spawn_host_pty(agent, params, slots, worktree, session, on_exit, true).await
}

async fn spawn_host_pty(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    worktree: &Path,
    session: HostPtySession<'_>,
    on_exit: Option<ExitHook>,
    without_inherited_tddy: bool,
) -> Result<Arc<PtyHandle>, SpecLaunchError> {
    let HostPtyPlan { argv, env } = plan_host_pty(agent, params, slots)?;
    let env: Vec<(String, String)> = env.into_iter().chain(session.env).collect();
    // TODO(env-allow-list): implement — the PTY child starts from
    // `agent.env_base.agent_env(&agent.spec, std::env::vars_os(), &env)`, not the daemon's
    // environment minus `TDDY_*` (`env -u` names, never values, on its argv).
    let argv = if without_inherited_tddy {
        without_inherited_tddy_variables(argv, &env)
    } else {
        argv
    };
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

/// `argv` run under `env -u NAME` for each `TDDY_*` variable the daemon's own environment holds and
/// `env` does not set: the agent process sees no `TDDY_*` variable the launch did not name
/// (`#agent-tools` stack rule). A PTY child inherits the daemon's environment, which a daemon
/// started from a dev shell or a test fills with its own `TDDY_*` configuration.
fn without_inherited_tddy_variables(argv: Vec<String>, env: &[(String, String)]) -> Vec<String> {
    let mut inherited: Vec<String> = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("TDDY_") && !env.iter().any(|(set, _)| set == name))
        .collect();
    if inherited.is_empty() {
        return argv;
    }
    inherited.sort();
    ["/usr/bin/env".to_string()]
        .into_iter()
        .chain(
            inherited
                .into_iter()
                .flat_map(|name| ["-u".to_string(), name]),
        )
        .chain(argv)
        .collect()
}

/// What a tmux pane runs before the agent, as `sh -c SCRIPT sh "<kept names>" <argv…>`: unset every
/// `TDDY_*` variable the pane holds except the space-separated names in `$1`, then `exec` the argv.
/// The names are read with a pattern that admits only shell names, so `unset` never fails.
const PANE_WITHOUT_INHERITED_TDDY: &str = r#"keep=" $1 "; shift; for name in $(/usr/bin/env | /usr/bin/sed -n 's/^\(TDDY_[A-Za-z0-9_]*\)=.*/\1/p'); do case "$keep" in *" $name "*) ;; *) unset "$name" ;; esac; done; exec "$@""#;

/// `argv` for a tmux pane, run without any `TDDY_*` variable but those `env` sets (`#agent-tools`
/// stack rule). A pane inherits the tmux *server's* environment — taken from whichever process
/// started that server, a daemon from a dev shell or a test among them — which the daemon cannot
/// read, so unlike [`without_inherited_tddy_variables`] the names are found in the pane itself.
pub(crate) fn in_a_pane_without_inherited_tddy(
    argv: Vec<String>,
    env: &[(String, String)],
) -> Vec<String> {
    let kept = env
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| name.starts_with("TDDY_"))
        .collect::<Vec<_>>()
        .join(" ");
    [
        "/bin/sh".to_string(),
        "-c".to_string(),
        PANE_WITHOUT_INHERITED_TDDY.to_string(),
        "sh".to_string(),
        kept,
    ]
    .into_iter()
    .chain(argv)
    .collect()
}

/// `argv` for a tmux pane, run with only the agent's environment (`#agent-tools 28/28`): `env` is
/// that whole environment, computed by the daemon (`CodingAgentSpec::agent_env`) and sourced from
/// the pane file. Of what the pane holds — the tmux server's environment plus that file — only the
/// names `env` sets are kept, every other variable is unset, then the argv is `exec`ed. Names only
/// reach the pane's argv, never values. Replaces [`in_a_pane_without_inherited_tddy`].
#[allow(dead_code)] // TODO(env-allow-list): the tmux harness launch calls this in place of the deny-list.
pub(crate) fn in_a_pane_with_only_the_agent_env(
    argv: Vec<String>,
    env: &[(String, String)],
) -> Vec<String> {
    // TODO(env-allow-list): implement — today's deny-list, kept so the build stands.
    in_a_pane_without_inherited_tddy(argv, env)
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

#[cfg(test)]
mod pane_with_only_the_agent_env_tests;

#[cfg(test)]
mod pane_without_inherited_tddy_tests {
    use super::in_a_pane_without_inherited_tddy;

    /// The `TDDY_*` names a command prints from its own environment.
    fn tddy_names_seen_by(argv: &[String], inherited: &[(&str, &str)]) -> Vec<String> {
        let output = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env_clear()
            .envs(inherited.iter().copied())
            .output()
            .expect("the pane command runs");
        let mut names: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
            .filter(|name| name.starts_with("TDDY_"))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_pane_drops_every_inherited_tddy_variable_and_keeps_the_ones_the_launch_sets() {
        // Given — a pane environment holding an inherited TDDY_ variable and one the launch set
        let argv = in_a_pane_without_inherited_tddy(
            vec!["/usr/bin/env".to_string()],
            &[
                ("TDDY_DAEMON_URL".to_string(), "http://daemon".to_string()),
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ],
        );

        // When
        let seen = tddy_names_seen_by(
            &argv,
            &[
                ("TDDY_LEAK_PROBE", "1"),
                ("TDDY_DAEMON_URL", "http://daemon"),
                ("PATH", "/usr/bin:/bin"),
            ],
        );

        // Then
        assert_eq!(seen, vec!["TDDY_DAEMON_URL".to_string()]);
    }
}
