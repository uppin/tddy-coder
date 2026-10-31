//! `--agent <spec id>`: the agent this app jails, chosen from the coding-agent registry
//! (`#agent-tools 10/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-agnostic-jail.md`. The app used to know four agents
//! by name (`--agent-kind`, `spawn::AgentKind`) and seed each one's home by hand. Here it knows
//! none: [`choose_agent`] loads the same registry the daemon does — the embedded specs with
//! `<tddyhome>/tools` overrides applied — and [`agent_exec_plan`] renders the chosen spec's argv and
//! slots for jail paths into the [`ExecPlan`] `tddy-sandbox-runner --exec-plan` runs. Nothing in
//! this module names an agent.

use std::fmt;
use std::path::{Path, PathBuf};

use tddy_actions::tool::{ToolDef, ToolError};
use tddy_coding_agent_specs::{CodingAgentSpec, SpecError};
use tddy_sandbox_runner::ExecPlan;

/// The directory under `<tddyhome>` whose tool files override the embedded ones.
pub const TOOLS_SUBDIR: &str = "tools";

/// The agent `--agent` names, resolved against the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChosenAgent {
    /// The spec id (`claude`, `cursor`, …).
    pub id: String,
    pub spec: CodingAgentSpec,
    /// The agent's own definition and each of its deps' — what the jail is lowered from.
    pub defs: Vec<ToolDef>,
}

/// The jail a chosen agent is launched into: the paths and choices the app owns, never the agent's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentJail {
    pub session_id: String,
    /// `None` runs the spec's default model.
    pub model: Option<String>,
    /// For a spec whose run template takes one; ignored by one that does not.
    pub permission_mode: String,
    /// Passed through verbatim, before the slot flags (whose MCP flag may be variadic).
    pub agent_args: Vec<String>,
    /// The jail's `$HOME` — what `{home}` in a placed slot resolves to.
    pub home: PathBuf,
    /// The jail's tmp dir — what `{tmp}` in an overlay resolves to.
    pub tmp: PathBuf,
    /// The read-only directory the agent's guidance files are in.
    pub context_dir: PathBuf,
    /// Where the agent starts: the mounted checkout, or the context dir for a managed codebase.
    pub cwd: PathBuf,
    /// The `tddy-tools` the agent's MCP slot registers, as the jail reaches it.
    pub tddy_tools_path: PathBuf,
    /// The socket in-jail `tddy-tools` relays tool calls to the host over.
    pub tool_ipc_socket: PathBuf,
}

/// Why `--agent` cannot start a jail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentChoiceError {
    /// The registry holds no coding agent with this id.
    Unknown { id: String, known: Vec<String> },
    /// The spec's route is `required_in_jail` and this app passes none — what `--agent-kind
    /// opencode|muse` was refused with.
    RouteRequired { id: String },
    /// The registry did not load.
    Registry(ToolError),
    /// The tool is not a usable coding agent, or a slot does not render.
    Spec(SpecError),
}

impl fmt::Display for AgentChoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { id, known } => write!(
                f,
                "--agent {id:?} is not a coding agent in the registry; it holds {}",
                known.join(", ")
            ),
            Self::RouteRequired { id } => write!(
                f,
                "--agent {id} is not started by tddy-sandbox-app yet: a jailed {id} session runs on \
                 a provider account, and this app passes none"
            ),
            Self::Registry(err) => write!(f, "the tool registry did not load: {err}"),
            Self::Spec(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AgentChoiceError {}

/// Choose the agent `id` from the registry at `tddy_home` (`<tddyhome>/`[`TOOLS_SUBDIR`] overrides
/// applied): refused for an id no coding agent has, naming every one the registry holds, and for a
/// spec whose route is required in a jail.
pub fn choose_agent(id: &str, tddy_home: &Path) -> Result<ChosenAgent, AgentChoiceError> {
    // TODO(jail-agnostic): implement — `load_agent_registry(<tddyhome>/tools)`,
    // `CodingAgentSpec::of`, `resolve_closure`, then refuse `route.required_in_jail`.
    let _ = (id, tddy_home);
    todo!("jail-agnostic: choose_agent")
}

/// The plan `tddy-sandbox-runner --exec-plan` runs for `agent` in `jail`: the run template and the
/// slots rendered for jail paths, the files the slots write, and the egress shim on.
pub fn agent_exec_plan(
    agent: &ChosenAgent,
    jail: &AgentJail,
) -> Result<ExecPlan, AgentChoiceError> {
    // TODO(jail-agnostic): implement — the template params `tddy-agent-launch` binds for a jailed
    // launch, `slots::render` with `SlotInputs.home = jail.home`, `render_argv`, and every written
    // slot file as an `ExecPlanFile`.
    let _ = (agent, jail);
    todo!("jail-agnostic: agent_exec_plan")
}
