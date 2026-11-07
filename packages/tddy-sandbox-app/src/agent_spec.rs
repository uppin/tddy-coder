//! `--agent <spec id>`: the agent this app jails, chosen from the coding-agent registry
//! (`#agent-tools 10/12`).
//!
//! Docs: `docs/ft/coder/sandbox-builder.md`. The app used to know four agents
//! by name (`--agent-kind`, `spawn::AgentKind`) and seed each one's home by hand. Here it knows
//! none: [`choose_agent`] loads the same registry the daemon does — the embedded specs with
//! `<tddyhome>/tools` overrides applied — and [`agent_exec_plan`] renders the chosen spec's argv and
//! slots for jail paths into the [`ExecPlan`] `tddy-sandbox-runner --exec-plan` runs. Nothing in
//! this module names an agent.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use tddy_actions::tool::{
    render_argv, ProcessToolHost, ResolvedTool, TemplateParams, ToolDef, ToolError, ToolHost,
    Transport,
};
use tddy_coding_agent_specs::claude_tools::{
    build_claude_allowlist, build_claude_disallowlist, PERMISSION_PROMPT_TOOL,
};
use tddy_coding_agent_specs::slots::{self, McpServerCommand, SlotError, SlotInputs};
use tddy_coding_agent_specs::{
    load_agent_registry, CodingAgentSpec, McpSlot, Placement, PlacementError, SpecError,
    CODING_AGENT_EXTENSION,
};
use tddy_sandbox_runner::{ExecPlan, ExecPlanFile};

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
    /// The agent's own tool as this host resolves it (`{binary}` in its run template).
    pub tool: ResolvedTool,
    /// Its deps as this host resolves them, deps first.
    pub deps: Vec<ResolvedTool>,
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
    /// The registry holds no coding agent with this id. `known` is every agent this app can run:
    /// those whose spec's `run` is a PTY launch.
    Unknown { id: String, known: Vec<String> },
    /// The agent's spec has no PTY `run` (an ACP-only agent), which a PTY sandbox cannot launch.
    /// `runnable` names the agents it can.
    NotPtyRunnable { id: String, runnable: Vec<String> },
    /// The spec's route is `required_in_jail` and this app passes none — what `--agent-kind
    /// opencode|muse` was refused with.
    RouteRequired { id: String },
    /// The registry did not load.
    Registry(ToolError),
    /// The tool is not a usable coding agent.
    Spec(SpecError),
    /// The agent's spec does not place it in a jail.
    Placement(PlacementError),
    /// A slot of the agent's spec does not render.
    Slot(SlotError),
}

impl fmt::Display for AgentChoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { id, known } => write!(
                f,
                "--agent {id:?} is not a coding agent in the registry; it holds {}",
                known.join(", ")
            ),
            Self::NotPtyRunnable { id, runnable } => write!(
                f,
                "--agent {id} has no PTY run, so tddy-sandbox-app cannot start it; it runs {}",
                runnable.join(", ")
            ),
            Self::RouteRequired { id } => write!(
                f,
                "--agent {id} is not started by tddy-sandbox-app yet: a jailed {id} session runs on \
                 a provider account, and this app passes none"
            ),
            Self::Registry(err) => write!(f, "the tool registry did not load: {err}"),
            Self::Spec(err) => write!(f, "{err}"),
            Self::Placement(err) => write!(f, "{err}"),
            Self::Slot(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AgentChoiceError {}

/// Choose the agent `id` from the registry at `tddy_home` (`<tddyhome>/`[`TOOLS_SUBDIR`] overrides
/// applied): refused for an id no coding agent has and for an ACP-only one, naming every agent with a
/// PTY run; for a spec that is not placed in a jail; and for one whose route is required in a jail.
pub fn choose_agent(id: &str, tddy_home: &Path) -> Result<ChosenAgent, AgentChoiceError> {
    choose_agent_on(id, tddy_home, &ProcessToolHost)
}

/// The default model of agent `id`'s spec, loaded the same way [`choose_agent`] loads the
/// registry, without resolving anything on this host: for a caller whose agent runs elsewhere
/// (the Linux daemon path), and so need not exist here at all.
pub fn agent_default_model(id: &str, tddy_home: &Path) -> Result<String, AgentChoiceError> {
    let registry =
        load_agent_registry(&tddy_home.join(TOOLS_SUBDIR)).map_err(AgentChoiceError::Registry)?;
    let mut known: Vec<String> = registry
        .ids()
        .filter(|tool| {
            registry
                .get(tool)
                .is_some_and(|def| def.extensions.contains_key(CODING_AGENT_EXTENSION))
        })
        .map(str::to_string)
        .collect();
    known.sort();
    let def = registry
        .get(id)
        .filter(|def| def.extensions.contains_key(CODING_AGENT_EXTENSION))
        .ok_or_else(|| AgentChoiceError::Unknown {
            id: id.to_string(),
            known,
        })?;
    let spec = CodingAgentSpec::of(def).map_err(AgentChoiceError::Spec)?;
    Ok(spec.agent.models.default.clone())
}

/// [`choose_agent`], with the agent and its deps resolved against `host` instead of this process's
/// environment and `PATH`.
pub fn choose_agent_on(
    id: &str,
    tddy_home: &Path,
    host: &dyn ToolHost,
) -> Result<ChosenAgent, AgentChoiceError> {
    let registry =
        load_agent_registry(&tddy_home.join(TOOLS_SUBDIR)).map_err(AgentChoiceError::Registry)?;
    let is_coding_agent = |def: &ToolDef| def.extensions.contains_key(CODING_AGENT_EXTENSION);
    let runs_in_a_pty = |def: &ToolDef| {
        def.run
            .as_ref()
            .is_some_and(|run| run.transport == Transport::Pty)
    };
    // The agents this app can run: coding agents whose spec's `run` is a PTY launch.
    let mut known: Vec<String> = registry
        .ids()
        .filter(|tool| {
            registry
                .get(tool)
                .is_some_and(|def| is_coding_agent(def) && runs_in_a_pty(def))
        })
        .map(str::to_string)
        .collect();
    known.sort();
    let def = match registry.get(id) {
        Some(def) if is_coding_agent(def) && runs_in_a_pty(def) => def,
        Some(def) if is_coding_agent(def) => {
            return Err(AgentChoiceError::NotPtyRunnable {
                id: id.to_string(),
                runnable: known,
            })
        }
        _ => {
            return Err(AgentChoiceError::Unknown {
                id: id.to_string(),
                known,
            })
        }
    };
    let spec = CodingAgentSpec::of(def).map_err(AgentChoiceError::Spec)?;
    spec.require_placement(Placement::Jail)
        .map_err(AgentChoiceError::Placement)?;
    if spec
        .agent
        .route
        .as_ref()
        .is_some_and(|route| route.required_in_jail)
    {
        return Err(AgentChoiceError::RouteRequired { id: id.to_string() });
    }

    let mut closure = registry
        .resolve_closure(&[id], host)
        .map_err(AgentChoiceError::Registry)?;
    // The closure lists deps first, so the agent's own tool is last.
    let tool = closure.pop().ok_or_else(|| AgentChoiceError::Unknown {
        id: id.to_string(),
        known: Vec::new(),
    })?;
    let defs = closure
        .iter()
        .chain([&tool])
        .filter_map(|resolved| registry.get(&resolved.id).cloned())
        .collect();
    Ok(ChosenAgent {
        id: id.to_string(),
        spec,
        defs,
        tool,
        deps: closure,
    })
}

/// The id of the dep every coding agent reaches its tools through.
pub(crate) const TOOL_SERVER_DEP: &str = "tddy-tools";

/// What the session adds to the launch beyond the jail's paths: the roster the agent's tool server
/// serves and the tools that roster takes away.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSession {
    /// The `env` block of the `tddy-tools --mcp` server, on top of its socket and session id.
    pub mcp_env: BTreeMap<String, String>,
    /// Whether a subagent is wired in, which decides whether its conversation tools are
    /// pre-approved.
    pub subagent_enabled: bool,
    /// The exec tools the session withdraws from the agent, each hard-disabled in both its forms.
    pub replaced_tools: Vec<String>,
}

/// The plan `tddy-sandbox-runner --exec-plan` runs for `agent` in `jail`: the run template and the
/// slots rendered for jail paths, the files the slots write, and the egress shim on.
pub fn agent_exec_plan(
    agent: &ChosenAgent,
    jail: &AgentJail,
) -> Result<ExecPlan, AgentChoiceError> {
    agent_exec_plan_with(agent, jail, &AgentSession::default())
}

/// [`agent_exec_plan`] for a session that wires a roster in: `session` adds to the tool server's
/// env and decides the tool lists an agent that takes them is launched with.
pub fn agent_exec_plan_with(
    agent: &ChosenAgent,
    jail: &AgentJail,
    session: &AgentSession,
) -> Result<ExecPlan, AgentChoiceError> {
    let spec = jailed(&agent.spec);
    let mut server_env = session.mcp_env.clone();
    server_env.insert(
        "TDDY_SANDBOX_TOOL_IPC".to_string(),
        jail.tool_ipc_socket.to_string_lossy().into_owned(),
    );
    server_env.insert(
        "TDDY_SANDBOX_SESSION_ID".to_string(),
        jail.session_id.clone(),
    );
    let server = McpServerCommand {
        name: TOOL_SERVER_DEP.to_string(),
        command: jail.tddy_tools_path.clone(),
        args: vec!["--mcp".to_string()],
        env: server_env,
        cwd: None,
    };
    // The lists are in the agent's own tool vocabulary, so an agent whose spec has no slot to take
    // them is given none, rather than refused for lists it never asked for.
    let takes_tool_lists = spec.agent.tool_allow.flag().is_some();
    let replaced: Vec<&str> = session.replaced_tools.iter().map(String::as_str).collect();
    let mut rendered = slots::render(
        &spec,
        &SlotInputs {
            workdir: jail.context_dir.clone(),
            scratch_dir: jail.tmp.clone(),
            mcp_server: Some(server),
            allowed_tools: if takes_tool_lists {
                build_claude_allowlist(session.subagent_enabled, &replaced)
            } else {
                Vec::new()
            },
            denied_tools: if takes_tool_lists {
                build_claude_disallowlist(&replaced)
            } else {
                Vec::new()
            },
            permission_prompt_tool: spec
                .agent
                .permission_prompt
                .flag()
                .map(|_| PERMISSION_PROMPT_TOOL.to_string()),
            preamble: None,
            append_system_prompt_file: None,
            hooks: None,
            home: Some(jail.home.clone()),
        },
    )
    .map_err(AgentChoiceError::Slot)?;
    rendered.argv_extra.splice(0..0, jail.agent_args.clone());

    let permission_mode = Some(jail.permission_mode.trim())
        .filter(|mode| !mode.is_empty())
        .unwrap_or("auto");
    let mut params = TemplateParams::new()
        .with("session_id", &jail.session_id)
        .with("permission_mode", permission_mode)
        .with_list("slots", rendered.argv_extra);
    if let Some(model) = jail.model.as_deref().filter(|model| !model.is_empty()) {
        params = params.with("model", model);
    }
    let argv = render_argv(&spec.run, &agent.tool, &params).map_err(AgentChoiceError::Registry)?;
    let env = spec
        .run
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    Ok(ExecPlan {
        argv,
        env,
        files: rendered
            .files
            .into_iter()
            .map(|(path, contents)| ExecPlanFile { path, contents })
            .collect(),
        egress_shim: true,
    })
}

/// `spec` as a jailed agent loads its MCP servers: the jail's context directory holds the project's
/// own MCP registration, which the agent has always loaded beside `tddy-tools`, so a strict flag —
/// which would drop it — is not passed (as the daemon's jailed launch does).
fn jailed(spec: &CodingAgentSpec) -> CodingAgentSpec {
    let mut spec = spec.clone();
    if let McpSlot::File { strict_flag, .. } = &mut spec.agent.mcp {
        *strict_flag = None;
    }
    spec
}
