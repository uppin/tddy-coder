//! The spec-driven host PTY launcher: a coding agent declared as a tool
//! (`tddy-coding-agent-specs`) is resolved on the host, its argv rendered from its `run:` template,
//! its slots written, and it is spawned in a PTY — with nothing agent-specific in this path.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`). The
//! unsandboxed `claude-cli` start and resume are its first callers; the jail (`#agent-tools 5/12`)
//! reuses the rendering, not this launcher.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::{
    ResolvedTool, TemplateParams, ToolError, ToolHost, ToolRegistry, Transport,
};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::{CodingAgentSpec, SpecError};

/// The coding agents this host can launch: the agent registry
/// (`tddy_coding_agent_specs::load_agent_registry`) and the host its tools resolve against.
///
/// Installed on the session host by the daemon runtime; a host without it launches `claude-cli`
/// the hand-built way (`TODO(claude-host)`: green makes this the only path).
pub struct HostCodingAgents {
    registry: ToolRegistry,
    host: Arc<dyn ToolHost>,
}

impl HostCodingAgents {
    pub fn new(registry: ToolRegistry, host: Arc<dyn ToolHost>) -> Self {
        Self { registry, host }
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Agent `id` resolved on this host: its spec, its own tool and the closure of its deps.
    /// `binary_override` — daemon.yaml `claude_cli.binary_path` until `#agent-tools 12/12` — wins
    /// over the spec's binary resolution when set.
    pub fn resolve(
        &self,
        id: &str,
        binary_override: Option<&Path>,
    ) -> Result<HostAgent, SpecLaunchError> {
        // TODO(claude-host): implement — `CodingAgentSpec::of(registry.get(id))`, then
        // `registry.resolve_closure(&[id], host)`, the override replacing the agent's own binary.
        let _ = (id, binary_override, &self.host);
        todo!("claude-host: HostCodingAgents::resolve")
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

/// Spawn `agent` in a PTY in `worktree`: refuse a spec whose `run.transport` is not `pty`, write
/// `slots.files`, render `run.argv` with `params` plus `slots.argv_extra` bound to the `slots`
/// list, and register the process under `session`.
pub async fn launch_host_pty(
    agent: &HostAgent,
    params: &TemplateParams,
    slots: &RenderedSlots,
    worktree: &Path,
    session: HostPtySession<'_>,
) -> Result<Arc<PtyHandle>, SpecLaunchError> {
    // TODO(claude-host): implement — needs a `CliSessionManager` entry that spawns a rendered argv
    // (`spawn_tool` is crate-private to `tddy-cli-sessions` today).
    let _ = (agent, params, slots, worktree, session);
    todo!("claude-host: launch_host_pty")
}
