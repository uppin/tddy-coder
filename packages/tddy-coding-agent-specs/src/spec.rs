//! The `coding_agent` extension: its schema, its owner, and the validated view of one agent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tddy_actions::tool::{
    render_argv, ArgvItem, ExtensionOwner, ResolvedTool, RunSection, TemplateParams, ToolDef,
    ToolError, ToolHost, Transport,
};
use tddy_agent_backend::adapters::{AcpHooksAdapters, AdapterRegistry, LaunchAdapters};
use tddy_agent_backend::AcpAgentCommand;
use thiserror::Error;

use crate::capability::Capability;
use crate::headless::{
    validate_headless, FreshSession, GoalParams, HeadlessProtocol, RouteModel, ScratchDir,
    SystemPromptDelivery,
};
use crate::route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent};
use crate::slots::conversation::ConversationSlot;
use crate::slots::hooks;
use crate::slots::tools::render_flag_per_value;

/// The `extensions:` key this crate owns.
pub const CODING_AGENT_EXTENSION: &str = "coding_agent";

/// The [`ExtensionOwner`] of [`CODING_AGENT_EXTENSION`]: a section that does not deserialize as a
/// [`CodingAgentSection`] (an unknown field or slot kind) or that names an unknown hooks adapter is
/// refused, so a bad agent file never loads.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodingAgentExtension;

impl ExtensionOwner for CodingAgentExtension {
    fn name(&self) -> &str {
        CODING_AGENT_EXTENSION
    }

    fn validate(&self, tool_id: &str, section: &serde_yaml::Value) -> Result<(), String> {
        // A spec whose `run.transport` is `acp` but which declares no `acp` section is refused where
        // an ACP launch reads the section: this seam sees the `coding_agent` section alone.
        // TODO(acp-agents): `ExtensionOwner::validate` would need the run section to refuse it here.
        let agent = parse_section(tool_id, section).map_err(|error| match error {
            SpecError::Invalid { message, .. } => message,
            other => other.to_string(),
        })?;
        if let Some(name) = agent
            .acp
            .as_ref()
            .and_then(|acp| acp.hooks_adapter.as_ref())
        {
            if !AcpHooksAdapters::builtin().contains(name) {
                return Err(format!(
                    "acp.hooks_adapter {name} is not an ACP hooks adapter this build has"
                ));
            }
        }
        let launch_adapters = LaunchAdapters::builtin();
        if let Some(name) = &agent.models.adapter {
            if launch_adapters.models(name).is_none() {
                return Err(format!(
                    "models.adapter {name} is not a launch adapter this build has"
                ));
            }
        }
        if agent.models.list_command.is_some() != agent.models.adapter.is_some() {
            return Err(
                "models.list_command and models.adapter are declared together or not at all"
                    .to_string(),
            );
        }
        if let Some(resume) = &agent.resume {
            if launch_adapters.resume_token(&resume.adapter).is_none() {
                return Err(format!(
                    "resume.adapter {} is not a launch adapter this build has",
                    resume.adapter
                ));
            }
        }
        match &agent.conversation {
            Some(ConversationSlot::Minted { adapter, .. })
                if launch_adapters.conversation_id(adapter).is_none() =>
            {
                return Err(format!(
                    "conversation.adapter {adapter} is not a launch adapter this build has"
                ));
            }
            Some(ConversationSlot::ResumeToken) if agent.resume.is_none() => {
                return Err(
                    "conversation kind resume_token needs a resume.adapter to read its token with"
                        .to_string(),
                );
            }
            _ => {}
        }
        if agent.placements.is_empty() {
            return Err(
                "placements is empty: an agent runs on at least one of host, jail".to_string(),
            );
        }
        if let Some(headless) = &agent.headless {
            validate_headless(tool_id, headless, &AdapterRegistry::builtin())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// One coding agent: the tool's id, its run section, and its `coding_agent` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentSpec {
    pub id: String,
    pub run: RunSection,
    pub agent: CodingAgentSection,
}

impl CodingAgentSpec {
    /// The coding-agent view of `def`: refused when it has no `run:` section or no
    /// `coding_agent` extension, or when that extension does not validate.
    pub fn of(def: &ToolDef) -> Result<Self, SpecError> {
        let run = def.run.clone().ok_or_else(|| SpecError::NoRunSection {
            tool: def.id.clone(),
        })?;
        let section = def.extensions.get(CODING_AGENT_EXTENSION).ok_or_else(|| {
            SpecError::NotACodingAgent {
                tool: def.id.clone(),
            }
        })?;
        Ok(Self {
            id: def.id.clone(),
            run,
            agent: parse_section(&def.id, section)?,
        })
    }
}

/// `section` as a [`CodingAgentSection`] whose hooks adapter this build has.
fn parse_section(
    tool_id: &str,
    section: &serde_yaml::Value,
) -> Result<CodingAgentSection, SpecError> {
    let invalid = |message: String| SpecError::Invalid {
        tool: tool_id.to_string(),
        message,
    };
    let agent: CodingAgentSection =
        serde_yaml::from_value(section.clone()).map_err(|e| invalid(e.to_string()))?;
    hooks::adapter(&agent.hooks.adapter).map_err(|e| invalid(e.to_string()))?;
    validate_env_entries(&agent.pass_env)
        .map_err(|e| format!("pass_env: {e}"))
        .map_err(invalid)?;
    Ok(agent)
}

/// Agent-env entries (`coding_agent.pass_env`, an agent-env base set) refused when one would let an
/// agent inherit a `TDDY_*` variable — `TDDY_X`, `TDDY_*`, or a prefix pattern every `TDDY_*` name
/// matches (`TDDY*`, `T*`, `*`) — or is not a variable name nor a `PREFIX_*` pattern. The message
/// names the entry.
pub(crate) fn validate_env_entries(entries: &[String]) -> Result<(), String> {
    for entry in entries {
        if admits_a_tddy_variable(entry) {
            return Err(format!(
                "env entry '{entry}' would admit a TDDY_* variable, which no agent inherits"
            ));
        }
        if !is_an_env_entry(entry) {
            return Err(format!(
                "env entry '{entry}' is neither a variable name nor a PREFIX_* pattern"
            ));
        }
    }
    Ok(())
}

/// Whether `entry` (a name or a pattern) could name a `TDDY_*` variable.
fn admits_a_tddy_variable(entry: &str) -> bool {
    const TDDY: &str = tddy_agent_backend::agent_env::TDDY_ENV_PREFIX;
    match entry.split_once('*') {
        // A prefix pattern admits every name starting with its prefix.
        Some((prefix, _)) => TDDY.starts_with(prefix) || prefix.starts_with(TDDY),
        None => entry == TDDY.trim_end_matches('_') || entry.starts_with(TDDY),
    }
}

/// Whether `entry` is a variable name (`PATH`) or a `PREFIX_*` pattern (`LC_*`).
fn is_an_env_entry(entry: &str) -> bool {
    let is_name = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    match entry.strip_suffix("_*") {
        Some(prefix) => is_name(&format!("{prefix}_")),
        None => is_name(entry),
    }
}

/// Why a tool is not a usable coding agent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpecError {
    #[error("tool {tool} has no coding_agent extension")]
    NotACodingAgent { tool: String },
    #[error("tool {tool} has no run section")]
    NoRunSection { tool: String },
    #[error("tool {tool}: coding_agent: {message}")]
    Invalid { tool: String, message: String },
}

/// The `coding_agent` section of an agent's tool file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingAgentSection {
    /// Where the tool server registration goes, and in what shape.
    pub mcp: McpSlot,
    /// The flag each pre-approved tool is passed with — or `unsupported`.
    pub tool_allow: ToolSlot,
    /// The flag each withdrawn tool is passed with — or `unsupported`.
    pub tool_deny: ToolSlot,
    /// The flag naming the MCP tool the agent asks before running anything not pre-approved — or
    /// `unsupported`.
    pub permission_prompt: ToolSlot,
    pub instructions: InstructionsSlot,
    pub hooks: HooksSlot,
    pub models: ModelsSection,
    /// What context sync copies from a codebase into the agent's working directory.
    #[serde(default)]
    pub context_globs: Vec<String>,
    /// How the agent runs headless — one invocation per turn, its output read by a named adapter
    /// (`#agent-tools 6/12`). `None` for an agent with no headless mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless: Option<HeadlessSection>,
    /// What a provider route needs of this agent (`#agent-tools 8/12`): whether a jailed launch
    /// requires one, and how the route's file and key reach the agent. `None` for an agent whose
    /// route is environment only and optional in a jail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteSlot>,
    /// How a resume finds the agent's own conversation (`#agent-tools 8/12`): the adapter that reads
    /// the token off the exited session's terminal. `None` for an agent resumed by an id the
    /// daemon owns (Claude's session id, Cursor's chat id) or by its worktree (`--continue`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeSlot>,
    /// Where the agent may run (`#agent-tools 8/12`). Absent means both; `[host]` refuses a
    /// sandboxed start up front, naming the placements ([`CodingAgentSpec::require_placement`]).
    #[serde(default = "Placement::anywhere")]
    pub placements: Vec<Placement>,
    /// Named ways of running this same agent with fixed extra flags and native tools withdrawn
    /// (`#agent-tools 10/12`) — `host_agent`, the agent a split session runs on this host against a
    /// codebase that lives elsewhere. Empty for an agent with none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, AgentVariant>,
    /// How the agent runs over the Agent Client Protocol (`#agent-tools 11/12`): its launch
    /// command, the hooks adapter its driver runs with, and how a managed launch withdraws its
    /// native tools. `None` for an agent with no ACP mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp: Option<AcpSection>,
    /// What a session of the agent offers (`#agent-tools 12/12`): every gate that used to compare
    /// a session type with `"claude-cli"` reads this instead ([`CodingAgentSpec::has`]). A word that
    /// is not a [`Capability`] is refused at load. Empty for an agent that offers none.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<Capability>,
    /// How long an orchestrated session's permission hook waits for its orchestrator's answer
    /// before the agent shows its own prompt, in seconds (`#agent-tools 12/12`; was daemon.yaml
    /// `claude_cli.answer_wait_secs`). [`DEFAULT_ANSWER_WAIT_SECS`] when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_wait_secs: Option<u64>,
    /// The operator-facing name a route refusal leads with (`#agent-tools 13/19`): `Claude Code`,
    /// `Muse Code`. Read through [`CodingAgentSpec::product`], which answers the id when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// How the session's own conversation is identified across its start and resumes
    /// (`#agent-tools 14/19`): the daemon's session id, a minted id, the worktree, or a resume
    /// token. Read through [`CodingAgentSpec::conversation`], which derives it when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationSlot>,
    /// The permission modes the agent's launch takes, in picker order — the values its argv's
    /// `{permission_mode}` binding may be given. Empty for an agent whose launch binds no
    /// permission mode. A picker offers these rather than naming an agent's modes in its own
    /// code (the sweep: agent metadata lives only in YAML).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_modes: Vec<String>,
    /// This agent leaves transcripts the host can total token usage from, when a transcript
    /// reader for its format exists. A summary that reads transcripts gates on this rather
    /// than naming the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transcript_usage: bool,
    /// The daemon's variables this agent's process is also started with, on top of the base set
    /// ([`crate::AgentEnvBase`]) — its provider keys and the agent-specific
    /// variables it reads (`#agent-tools 28/28`). Each entry is a name or a `PREFIX_*` pattern; one
    /// matching `TDDY_*` is refused at load. Empty for an agent that reads none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_env: Vec<String>,
}

/// How long an orchestrated session's permission hook waits for its orchestrator's answer when the
/// agent's spec sets no `coding_agent.answer_wait_secs`.
pub const DEFAULT_ANSWER_WAIT_SECS: u64 = 120;

impl CodingAgentSpec {
    /// `coding_agent.answer_wait_secs`, or [`DEFAULT_ANSWER_WAIT_SECS`].
    pub fn answer_wait_secs(&self) -> u64 {
        self.agent
            .answer_wait_secs
            .unwrap_or(DEFAULT_ANSWER_WAIT_SECS)
    }

    /// The capabilities the agent declares.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.agent.capabilities
    }

    /// Whether the agent declares `capability` — the question every agent gate asks.
    pub fn has(&self, capability: Capability) -> bool {
        self.agent.capabilities.contains(&capability)
    }

    /// What a provider route can do with the agent: `coding_agent.route.providers` and `.shape`, or
    /// no providers when the spec has no `route` section — an agent that cannot be routed.
    pub fn route_support(&self) -> RouteSupport {
        match &self.agent.route {
            Some(route) => RouteSupport {
                providers: route.providers.clone(),
                shape: route.shape,
            },
            None => RouteSupport::default(),
        }
    }

    /// `coding_agent.conversation`; when the spec declares none, [`ConversationSlot::ResumeToken`]
    /// for a spec with a `resume` slot and [`ConversationSlot::Worktree`] otherwise — what the
    /// generic launch resumed by before the slot existed.
    pub fn conversation(&self) -> ConversationSlot {
        match (&self.agent.conversation, &self.agent.resume) {
            (Some(declared), _) => declared.clone(),
            (None, Some(_)) => ConversationSlot::ResumeToken,
            (None, None) => ConversationSlot::Worktree,
        }
    }

    /// `coding_agent.pass_env`: the names and `PREFIX_*` patterns of the daemon variables the
    /// agent's process is also started with.
    pub fn pass_env(&self) -> &[String] {
        &self.agent.pass_env
    }

    /// The variables of `parent` (the daemon's environment) that [`Self::pass_env`] names, in
    /// `parent`'s order — what every host launch of this agent adds to its environment. A variable
    /// whose name or value is not UTF-8 is not passed.
    pub fn passed_env(
        &self,
        parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    ) -> Vec<(String, String)> {
        parent
            .into_iter()
            .filter(|(name, _)| {
                self.agent.pass_env.iter().any(|pattern| {
                    tddy_agent_backend::agent_env::matches_env_pattern(pattern, name)
                })
            })
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect()
    }

    /// `coding_agent.product`, or the agent id when the spec declares none.
    pub fn product(&self) -> &str {
        self.agent.product.as_deref().unwrap_or(&self.id)
    }

    /// The parameter names the spec's `run.argv` template binds — `{name}` placeholders, `if:`
    /// conditions and `splice:` lists, recursively. The argv template is the agent's CLI arg
    /// schema, so this is read from it rather than restated: a caller asks whether the launch
    /// takes a `permission_mode`, a `model`, a `chat_id`, without naming the agent.
    pub fn argv_params(&self) -> std::collections::BTreeSet<String> {
        fn placeholders(text: &str, into: &mut std::collections::BTreeSet<String>) {
            let mut rest = text;
            while let Some(open) = rest.find('{') {
                let Some(close) = rest[open..].find('}') else {
                    return;
                };
                let name = &rest[open + 1..open + close];
                if !name.is_empty() {
                    into.insert(name.to_string());
                }
                rest = &rest[open + close + 1..];
            }
        }
        fn walk(items: &[ArgvItem], into: &mut std::collections::BTreeSet<String>) {
            for item in items {
                match item {
                    ArgvItem::Lit(text) => placeholders(text, into),
                    ArgvItem::When { if_, then, else_ } => {
                        into.insert(if_.clone());
                        walk(then, into);
                        walk(else_, into);
                    }
                    ArgvItem::Splice { splice } => {
                        into.insert(splice.clone());
                    }
                }
            }
        }
        let mut params = std::collections::BTreeSet::new();
        walk(&self.run.argv, &mut params);
        params
    }

    /// Whether the spec's `run.argv` binds `param` (see [`Self::argv_params`]).
    pub fn binds_argv_param(&self, param: &str) -> bool {
        self.argv_params().contains(param)
    }

    /// A resolved-tool view of `binary` for rendering this spec's argv templates with it,
    /// without resolving anything on a host.
    fn resolved_tool(&self, binary: &std::path::Path) -> ResolvedTool {
        ResolvedTool {
            id: self.id.clone(),
            command: binary
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            binary: binary.to_path_buf(),
            reads: Vec::new(),
            network: tddy_actions::tool::NetworkSection::default(),
        }
    }

    /// The spec's `run.argv` rendered with `binary`: the argv a PTY launch of this agent
    /// spawns, for a caller that resolved the binary itself. `values` binds scalar template
    /// parameters, `flags` the ones that are simply on, and `slots` splices as an empty list
    /// (a caller with slot arguments renders through its launch slots instead).
    pub fn render_run_argv(
        &self,
        binary: &std::path::Path,
        values: &[(&str, &str)],
        flags: &[&str],
    ) -> Result<Vec<String>, SpecError> {
        let mut params = TemplateParams::new().with_list("slots", Vec::new());
        for (name, value) in values {
            if !value.is_empty() {
                params = params.with(name, value);
            }
        }
        for flag in flags {
            params = params.with_flag(flag, true);
        }
        render_argv(&self.run, &self.resolved_tool(binary), &params).map_err(|e| {
            SpecError::Invalid {
                tool: self.id.clone(),
                message: e.to_string(),
            }
        })
    }

    /// The agent's model-listing command (`coding_agent.models.list_command`) rendered with
    /// `binary`, and the adapter that reads its stdout. `None` for an agent whose catalogue is
    /// only `curated`. The binary is bound as `{binary}` without resolving it on a host, so a
    /// caller that already resolved it (a daemon probe) renders the listing for that binary.
    pub fn model_listing_argv(
        &self,
        binary: &std::path::Path,
    ) -> Option<Result<(Vec<String>, String), SpecError>> {
        let (list_command, adapter) = (
            self.agent.models.list_command.as_ref()?,
            self.agent.models.adapter.clone()?,
        );
        let tool = self.resolved_tool(binary);
        let section = RunSection {
            transport: Transport::Stdio,
            argv: list_command.clone(),
            env: std::collections::BTreeMap::new(),
        };
        Some(
            render_argv(&section, &tool, &TemplateParams::new())
                .map(|argv| (argv, adapter))
                .map_err(|e| SpecError::Invalid {
                    tool: self.id.clone(),
                    message: e.to_string(),
                }),
        )
    }

    /// The curated catalogue: `(id, label)` pairs and the default model id.
    pub fn curated_models(&self) -> (Vec<(String, String)>, String) {
        (
            self.agent
                .models
                .curated
                .iter()
                .map(|model| (model.id().to_string(), model.label().to_string()))
                .collect(),
            self.agent.models.default.clone(),
        )
    }
}

/// What a route body knows of `spec` (`#agent-tools 13/19`): its id, its product, its route
/// support and its `route.key_env`. A spec without a `route` section gives an agent with no
/// providers, which every route body refuses.
pub fn routed_agent(spec: &CodingAgentSpec) -> RoutedAgent {
    RoutedAgent {
        id: spec.id.clone(),
        product: spec.product().to_string(),
        support: spec.route_support(),
        key_env: spec
            .agent
            .route
            .as_ref()
            .and_then(|route| route.key_env.clone()),
    }
}

/// `coding_agent.acp`: one ACP launch of the agent (`#agent-tools 11/12`).
///
/// The command is rendered with `tddy_actions::tool::render_argv` over [`Self::run_section`] —
/// `{binary}` bound to the resolved binary, `route_args` to the provider route's arguments. The
/// tool server reaches the agent through the MCP slot (`acp-session`: `session/new`
/// `mcp_servers`), and a managed launch withdraws [`Self::withdraw_native`] through the deny slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcpSection {
    /// The argv template of the ACP agent process.
    pub command: Vec<ArgvItem>,
    /// Set on the process on top of what the launcher passes. Values are literal.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The hooks adapter (`tddy_agent_backend::adapters::AcpHooksAdapters`) the driver runs with —
    /// `codex-acp-login`, `opencode-route-file`; `None` runs it with no hooks. A name the registry
    /// does not hold is refused at load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks_adapter: Option<String>,
    /// Where an ACP launch registers its MCP server, when that differs from `coding_agent.mcp`
    /// (OpenCode: none on its PTY launch, `session/new` here). `None` uses `coding_agent.mcp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpSlot>,
    /// How an ACP launch withdraws native tools, when that differs from `coding_agent.tool_deny`
    /// (OpenCode: `config_env` here, `unsupported` on its PTY launch). `None` uses
    /// `coding_agent.tool_deny`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_deny: Option<ToolSlot>,
    /// The agent's own file-system and shell tools, by the names its deny slot takes — what a
    /// managed launch withdraws. Empty for an agent that cannot withdraw any.
    #[serde(default)]
    pub withdraw_native: Vec<String>,
}

impl AcpSection {
    /// This section's command as an `acp` run section, for `render_argv`.
    pub fn run_section(&self) -> RunSection {
        RunSection {
            transport: Transport::Acp,
            argv: self.command.clone(),
            env: self.env.clone(),
        }
    }
}

/// Why an agent's ACP command cannot be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AcpCommandError {
    #[error("agent {agent} declares no acp section, so it has no ACP command")]
    NoAcpSection { agent: String },
    #[error("agent {agent}'s acp command renders no program")]
    EmptyCommand { agent: String },
    #[error(transparent)]
    Render(#[from] ToolError),
}

/// Whether `def`'s binary was not named by its `binary.env` on `host`: it was found by the `PATH`
/// search for `binary.command`, as `bunx` is. A `coding_agent.acp.command` that runs a package
/// through a runner renders `package_runner` from this.
pub fn package_runner(def: &ToolDef, host: &dyn ToolHost) -> bool {
    match def.binary.env.as_deref() {
        Some(var) => host.env_var(var).is_none(),
        None => true,
    }
}

impl CodingAgentSpec {
    /// The agent's ACP declaration, if it has one.
    pub fn acp(&self) -> Option<&AcpSection> {
        self.agent.acp.as_ref()
    }

    /// The command an ACP launch of this agent runs: its `coding_agent.acp.command` rendered with
    /// `tool` (the agent's resolved binary) as `{binary}`, `package_runner` and `route_args` bound,
    /// and the section's own `env`. The first rendered entry is the program.
    pub fn acp_command(
        &self,
        tool: &ResolvedTool,
        package_runner: bool,
        route_args: Vec<String>,
    ) -> Result<AcpAgentCommand, AcpCommandError> {
        let acp = self.acp().ok_or_else(|| AcpCommandError::NoAcpSection {
            agent: self.id.clone(),
        })?;
        let params = TemplateParams::new()
            .with_flag("package_runner", package_runner)
            .with_list("route_args", route_args);
        let argv = render_argv(&acp.run_section(), tool, &params)?;
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| AcpCommandError::EmptyCommand {
                agent: self.id.clone(),
            })?;
        // `env` is the launch env (`acp.env`): the launcher computes the agent's whole
        // environment from it with `AgentEnvBase::agent_env` (`#agent-tools 28/28`).
        Ok(AcpAgentCommand {
            program: PathBuf::from(program),
            args: args.to_vec(),
            env: acp
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            cwd: None,
        })
    }

    /// The MCP slot an ACP launch registers its tool server through: `acp.mcp`, else
    /// `coding_agent.mcp`. `None` for an agent with no ACP mode.
    pub fn acp_mcp(&self) -> Option<&McpSlot> {
        let acp = self.acp()?;
        Some(acp.mcp.as_ref().unwrap_or(&self.agent.mcp))
    }

    /// The deny slot an ACP launch withdraws native tools through: `acp.tool_deny`, else
    /// `coding_agent.tool_deny`. `None` for an agent with no ACP mode.
    pub fn acp_tool_deny(&self) -> Option<&ToolSlot> {
        let acp = self.acp()?;
        Some(acp.tool_deny.as_ref().unwrap_or(&self.agent.tool_deny))
    }
}

/// The variant a split session's agent runs as: on this host, against a codebase on another daemon
/// or in a jail, so nothing from the checkout may configure it and none of its native filesystem or
/// shell tools may run (`#agent-tools 10/12`).
pub const HOST_AGENT_VARIANT: &str = "host_agent";

/// `coding_agent.variants.<name>`: what a variant adds to the agent's ordinary launch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVariant {
    /// Fixed flags, appended after the run template's own and before the slot flags.
    #[serde(default)]
    pub args: Vec<String>,
    /// The agent's own tools the variant withdraws, each passed through the `tool_deny` slot.
    #[serde(default)]
    pub withdraw_native: Vec<String>,
}

/// Why a variant cannot be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VariantError {
    #[error("agent {agent} declares no variant {variant:?}; it declares {declared:?}")]
    Unknown {
        agent: String,
        variant: String,
        declared: Vec<String>,
    },
    #[error(
        "agent {agent}: variant {variant:?} withdraws native tools, but its tool_deny slot is unsupported"
    )]
    DenyUnsupported { agent: String, variant: String },
}

impl CodingAgentSpec {
    /// The flags variant `name` adds to a launch: its `args`, then `tool_deny`'s flag once per
    /// `withdraw_native` tool. Refused for a variant the agent does not declare, naming the ones it
    /// does, and for one that withdraws tools through an `unsupported` deny slot.
    pub fn variant_args(&self, name: &str) -> Result<Vec<String>, VariantError> {
        let variant = self
            .agent
            .variants
            .get(name)
            .ok_or_else(|| VariantError::Unknown {
                agent: self.id.clone(),
                variant: name.to_string(),
                declared: self.agent.variants.keys().cloned().collect(),
            })?;
        let mut args = variant.args.clone();
        if !variant.withdraw_native.is_empty() {
            let deny =
                self.agent
                    .tool_deny
                    .flag()
                    .ok_or_else(|| VariantError::DenyUnsupported {
                        agent: self.id.clone(),
                        variant: name.to_string(),
                    })?;
            args.extend(render_flag_per_value(deny, &variant.withdraw_native));
        }
        Ok(args)
    }
}

impl CodingAgentSpec {
    /// The agent's headless declaration, if it has one.
    pub fn headless(&self) -> Option<&HeadlessSection> {
        self.agent.headless.as_ref()
    }
}

impl CodingAgentSpec {
    /// `Ok` when the agent's `placements` include `placement`; otherwise the refusal, naming the
    /// agent and every placement it declares — a sandboxed `warp-cli` start is refused with it
    /// (`#agent-tools 8/12`).
    pub fn require_placement(&self, placement: Placement) -> Result<(), PlacementError> {
        if self.agent.placements.contains(&placement) {
            return Ok(());
        }
        Err(PlacementError::NotPlaced {
            tool: self.id.clone(),
            placement,
            placements: self.agent.placements.clone(),
        })
    }
}

/// `coding_agent.route`: what a provider route needs of the agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSlot {
    /// A jailed launch without a provider route is refused: the jail holds no credentials of the
    /// agent's own (OpenCode, Muse).
    #[serde(default)]
    pub required_in_jail: bool,
    /// The variable the route's first file is named in (`OPENCODE_CONFIG`); `None` for a route that
    /// is environment only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_env: Option<String>,
    /// The directory under the jail's scratch the route's files are written to owner-only
    /// (`provider-route`); `None` for a route with no files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_dir: Option<String>,
    /// The variable the route's key is handed in (`OPENCODE_PROVIDER_KEY`, `META_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// The providers a route can point the agent at (`#agent-tools 12/12`); empty for an agent that
    /// always runs on its own vendor. Read through [`CodingAgentSpec::route_support`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderKind>,
    /// How the route reaches the agent (`#agent-tools 12/12`): environment (the default),
    /// `-c` TOML arguments (`toml_args`), or a JSON configuration file (`json_config_file`).
    #[serde(default)]
    pub shape: RouteShape,
}

/// `coding_agent.resume`: the launch adapter (`tddy_agent_backend::adapters::LaunchAdapters`) that
/// reads a resume token off an exited session's terminal — `warp-resume-token`. A name the registry
/// does not hold is refused at load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeSlot {
    pub adapter: String,
}

/// Where an agent may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// A PTY on the daemon's host, as the session's OS user.
    Host,
    /// Inside a session jail, through `tddy-sandbox-runner`.
    Jail,
}

impl Placement {
    /// The placements of a spec that declares none: both.
    pub fn anywhere() -> Vec<Placement> {
        vec![Placement::Host, Placement::Jail]
    }

    /// The word a spec writes it as.
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Host => "host",
            Placement::Jail => "jail",
        }
    }
}

impl std::fmt::Display for Placement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an agent cannot run where a launch asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlacementError {
    /// The spec's `placements` exclude the asked one.
    #[error(
        "{tool} cannot run in the {placement}: its spec's placements are [{}]",
        placements.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
    )]
    NotPlaced {
        tool: String,
        placement: Placement,
        placements: Vec<Placement>,
    },
}

/// `coding_agent.headless`: one headless turn's argv template, its static env, the adapter its
/// output is read through, and whether a non-zero exit with a structured result is a success.
///
/// The argv is rendered with `tddy_actions::tool::render_argv` over [`Self::run_section`], the
/// slots spliced in as `slots` — the same grammar and slots as the PTY `run:` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadlessSection {
    pub argv: Vec<ArgvItem>,
    /// Set on the process on top of what the launcher passes. Values are literal.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The stream adapter (`tddy_agent_backend::adapters::AdapterRegistry`) that reads the
    /// agent's stdout: `claude-stream-json`. A name the registry does not hold is refused at load.
    pub adapter: String,
    /// A non-zero exit whose result carries a structured response counts as success. A property of
    /// the agent, not of the goal: it replaces
    /// `GoalHints::claude_nonzero_exit_ok_if_structured_response`.
    #[serde(default)]
    pub exit_ok_if_structured: bool,
    /// Where a headless run registers its MCP server, when that differs from the agent's
    /// `coding_agent.mcp` (Cursor: a project file in the worktree headless, `{home}` for its PTY
    /// session). `None` uses `coding_agent.mcp`. Added by the 2026-10-31 amendment.
    #[serde(default)]
    pub mcp: Option<McpSlot>,
    // --- The workflow turn's protocol (`#agent-tools 15/19`): what a workflow turn of the agent
    // needs beyond its argv, read by `SpecHeadlessBackend` through [`Self::protocol`]. Each is named
    // after what it does, never after an agent; an unknown word is refused at load.
    /// How the workflow's system prompt reaches the agent: `flag` (the `append_system_prompt` slot)
    /// or `prepend` (in front of the prompt). `prepend` when absent.
    #[serde(default)]
    pub system_prompt: SystemPromptDelivery,
    /// The template params a goal's hints bind: `always`, `plan_mode`, `read_only_plan`,
    /// `accept_edits`, each a map of param name to value.
    #[serde(default)]
    pub goal_params: GoalParams,
    /// The workflow binds the approval tool (`mcp__tddy-tools__approval_prompt`) when it registers
    /// the tool server.
    #[serde(default)]
    pub approval_tool: bool,
    /// The workflow registers the agent's tool server (its `tddy-tools` dep with `--mcp`).
    #[serde(default)]
    pub workflow_mcp: bool,
    /// Whether a provider route's argv replaces the request's model: `{ kind: kept }`,
    /// `{ kind: replaced }` or `{ kind: replaced_if_route_has, flag: <flag> }`. `kept` when absent.
    #[serde(default)]
    pub route_model: RouteModel,
    /// Who names a fresh session's id: `caller` (passed as `session_id`) or `agent`. `agent` when
    /// absent.
    #[serde(default)]
    pub fresh_session: FreshSession,
    /// Where the turn's registration files go: `session_dir` or `workdir`. `session_dir` when
    /// absent.
    #[serde(default)]
    pub scratch: ScratchDir,
    /// Who answers a headless conversation's permission requests (`#agent-tools 27/28`):
    /// `asks_caller`, or `{ fixed: <text> }` for a CLI that cannot ask mid-turn and runs under a
    /// fixed policy, the text shown on its catalog row. `None` when the spec declares none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<HeadlessPermissionPolicy>,
}

/// `coding_agent.headless.permission_policy`: who answers a headless conversation's permission
/// requests. Spelled `asks_caller` or `{ fixed: <text> }` (an untagged YAML shape, since YAML enums
/// are otherwise tagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PermissionPolicyYaml", into = "PermissionPolicyYaml")]
pub enum HeadlessPermissionPolicy {
    /// The request yields to the caller (the main agent).
    AsksCaller,
    /// The CLI cannot ask mid-turn; it runs under this policy, described for the catalog row.
    Fixed(String),
}

/// [`HeadlessPermissionPolicy`] as its YAML is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PermissionPolicyYaml {
    Word(AsksCallerWord),
    Fixed { fixed: String },
}

/// The one bare word `permission_policy` takes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AsksCallerWord {
    AsksCaller,
}

impl From<PermissionPolicyYaml> for HeadlessPermissionPolicy {
    fn from(yaml: PermissionPolicyYaml) -> Self {
        match yaml {
            PermissionPolicyYaml::Word(AsksCallerWord::AsksCaller) => Self::AsksCaller,
            PermissionPolicyYaml::Fixed { fixed } => Self::Fixed(fixed),
        }
    }
}

impl From<HeadlessPermissionPolicy> for PermissionPolicyYaml {
    fn from(policy: HeadlessPermissionPolicy) -> Self {
        match policy {
            HeadlessPermissionPolicy::AsksCaller => Self::Word(AsksCallerWord::AsksCaller),
            HeadlessPermissionPolicy::Fixed(fixed) => Self::Fixed { fixed },
        }
    }
}

impl HeadlessSection {
    /// The protocol a workflow turn of this agent runs with: this section's protocol fields.
    pub fn protocol(&self) -> HeadlessProtocol {
        HeadlessProtocol {
            system_prompt: self.system_prompt,
            goal_params: self.goal_params.clone(),
            approval_tool: self.approval_tool,
            workflow_mcp: self.workflow_mcp,
            route_model: self.route_model.clone(),
            fresh_session: self.fresh_session,
            scratch: self.scratch,
        }
    }

    /// This section as a `stdio` run section, for `render_argv`.
    pub fn run_section(&self) -> RunSection {
        RunSection {
            transport: Transport::Stdio,
            argv: self.argv.clone(),
            env: self.env.clone(),
        }
    }
}

/// Where an agent takes its MCP server registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum McpSlot {
    /// A file in `format`, passed by `flag <path>`, then `strict_flag` when the spec declares one.
    File {
        flag: String,
        format: McpFormat,
        /// Loads the registered servers and no other MCP configuration (`--strict-mcp-config`),
        /// passed after the path. Optional: an agent without such a flag omits it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict_flag: Option<String>,
    },
    /// A file in `format` written where the agent looks for it, passed by no flag
    /// (`#agent-tools 7/12`: Cursor reads `$HOME/.cursor/mcp.json`). `path` may name `{home}` — the
    /// agent's home on this launch ([`crate::slots::SlotInputs::home`]); every other character is
    /// literal.
    Placed { path: String, format: McpFormat },
    /// The agent takes no MCP registration (`#agent-tools 8/12`: OpenCode, Muse and Warp on
    /// today's launch). A launch that passes a tool server is refused rather than dropped, as an
    /// `unsupported` tool slot is.
    Unsupported,
    /// The registration as argv config overrides, with no file (`#agent-tools 9/12`: Codex's
    /// `-c mcp_servers.<name>.command=…`). Each field is `flag <key_prefix>.<name>.<field>=<toml>`
    /// for `command`, `args` and `env_vars`. The server's environment is **named** (`env_vars`),
    /// never valued: the agent forwards those variables from its own environment, so the
    /// credential stays off the argv another user's `ps` reads.
    Inline { flag: String, key_prefix: String },
    /// A file passed by `flag <path>` in the servers-by-name shape `{"<name>": {command, args,
    /// env}}` (`#agent-tools 9/12`: Oz's `--mcp <path>`), written owner-only under the launch's
    /// scratch directory as [`crate::slots::headless_mcp::ARGV_MCP_FILE`].
    Argv { flag: String },
    /// A file in `format` written at `path` under the agent's working directory, passed by no flag
    /// (`#agent-tools 9/12`: a headless Cursor reads its worktree's `.cursor/mcp.json`). The file
    /// is the conversation's, so it is listed in `RenderedSlots::written` and removed when the
    /// conversation closes or the start-up sweep finds it.
    Workdir { path: PathBuf, format: McpFormat },
    /// The registration as ACP `session/new` (and `session/load`) `mcp_servers`, with no file and
    /// no flag (`#agent-tools 11/12`: every ACP agent). Rendered by
    /// [`crate::slots::acp::acp_mcp_servers`].
    AcpSession,
}

/// The shape of an MCP registration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpFormat {
    /// `{"mcpServers": {"<name>": {"command", "args", "env"?}}}`.
    ClaudeMcpJson,
    /// Cursor's `.cursor/mcp.json`: the same `mcpServers` object, at the placed path
    /// (`#agent-tools 7/12`).
    CursorMcpJson,
}

/// A tool slot: the flag an agent takes it by, or the declaration that it has none.
///
/// `tool_deny: unsupported` is how a spec says "this agent cannot withdraw its native tools" (Cursor,
/// Codex, Oz). It is spec data rather than an absent field, so the gap is reviewable in the YAML and
/// a caller that needs the slot is refused by name instead of silently getting nothing.
///
/// An ACP agent takes its withdrawn tools another way (`#agent-tools 11/12`), each written as a map
/// keyed by its kind: `{ acp_meta: <dotted path> }` — a list at that path of `session/new` `_meta`
/// (Claude's `claudeCode.options.disallowedTools`) — and `{ config_env: <VAR> }` — inline JSON
/// config `{"tools": {<tool>: false}}` in that variable (OpenCode's `OPENCODE_CONFIG_CONTENT`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSlot {
    Flag(FlagSlot),
    Unsupported(Unsupported),
    /// The tools as a list at `path` (dotted) in the ACP `session/new` `_meta`.
    AcpMeta {
        #[serde(rename = "acp_meta")]
        path: String,
    },
    /// The tools switched off in inline JSON config handed in variable `env`.
    ConfigEnv {
        #[serde(rename = "config_env")]
        env: String,
    },
}

/// The YAML word `unsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unsupported {
    #[serde(rename = "unsupported")]
    Unsupported,
}

impl ToolSlot {
    /// The flag, when the agent has one.
    pub fn flag(&self) -> Option<&FlagSlot> {
        match self {
            ToolSlot::Flag(flag) => Some(flag),
            ToolSlot::Unsupported(_) | ToolSlot::AcpMeta { .. } | ToolSlot::ConfigEnv { .. } => {
                None
            }
        }
    }
}

/// A slot rendered as `flag <value>` argv pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagSlot {
    pub flag: String,
}

/// Where an agent reads its instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionsSlot {
    /// Context files a launch's preamble is prepended to, relative to the agent's working
    /// directory; one the directory lacks is left alone.
    pub files: Vec<PathBuf>,
    /// The flag a system-prompt file is appended with; `None` for an agent without one.
    #[serde(default)]
    pub append_system_prompt: Option<FlagSlot>,
    /// A rules file, relative to the agent's working directory, that a launch's preamble is
    /// written to **whole** (`#agent-tools 7/12`: Cursor's `.cursor/rules/<name>.mdc`); `None` for
    /// an agent without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_file: Option<PathBuf>,
}

/// Which Rust hooks adapter writes the agent's lifecycle-hooks configuration. The hook protocol is
/// not declarable; a file can only name an adapter (`claude-settings-local`, `cursor-hooks-json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksSlot {
    pub adapter: String,
}

/// One curated model: its id, and the label a picker shows for it. Written in YAML either as the
/// bare id (`opus`, whose label is the id) or as `{ id: opus, label: "Claude Opus (latest)" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CuratedModel {
    /// The bare id; its label is the id.
    Id(String),
    /// The id with its picker's label.
    Labelled { id: String, label: String },
}

impl CuratedModel {
    /// The model id passed to the agent.
    pub fn id(&self) -> &str {
        match self {
            CuratedModel::Id(id) => id,
            CuratedModel::Labelled { id, .. } => id,
        }
    }

    /// The label a picker shows; the id for a bare entry.
    pub fn label(&self) -> &str {
        match self {
            CuratedModel::Id(id) => id,
            CuratedModel::Labelled { label, .. } => label,
        }
    }
}

/// The models an agent offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsSection {
    pub default: String,
    pub curated: Vec<CuratedModel>,
    /// The command that lists the agent's models (`["{binary}", "models"]`), rendered with the run
    /// grammar; `None` for an agent whose catalogue is `curated` (`#agent-tools 8/12`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_command: Option<Vec<ArgvItem>>,
    /// The launch adapter (`tddy_agent_backend::adapters::LaunchAdapters`) that reads
    /// `list_command`'s stdout — `opencode-models`. Set exactly when `list_command` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[cfg(test)]
mod tool_slot_tests {
    use pretty_assertions::assert_eq;

    use super::{FlagSlot, ToolSlot, Unsupported};

    #[test]
    fn a_tool_slot_reads_a_flag_map() {
        // When
        let slot: ToolSlot = serde_yaml::from_str("{ flag: --disallowedTools }").unwrap();

        // Then
        assert_eq!(
            slot,
            ToolSlot::Flag(FlagSlot {
                flag: "--disallowedTools".to_string(),
            })
        );
    }

    #[test]
    fn a_tool_slot_reads_the_word_unsupported() {
        // When
        let slot: ToolSlot = serde_yaml::from_str("unsupported").unwrap();

        // Then
        assert_eq!(slot, ToolSlot::Unsupported(Unsupported::Unsupported));
        assert_eq!(slot.flag(), None);
    }

    #[test]
    fn any_other_word_is_refused() {
        // When
        let refused = serde_yaml::from_str::<ToolSlot>("none");

        // Then
        assert!(refused.is_err(), "`none` must not read as a tool slot");
    }
}

#[cfg(test)]
mod placement_tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::RunSection;

    use super::{CodingAgentSection, CodingAgentSpec, Placement, PlacementError};

    /// The coding-agent view of a built-in spec file, read straight from its YAML: its `run` and
    /// `extensions.coding_agent` sections deserialized as they are declared.
    fn the_builtin_spec(id: &str, yaml: &str) -> CodingAgentSpec {
        let document: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let run: RunSection = serde_yaml::from_value(document["run"].clone()).unwrap();
        let agent: CodingAgentSection =
            serde_yaml::from_value(document["extensions"]["coding_agent"].clone()).unwrap();
        CodingAgentSpec {
            id: id.to_string(),
            run,
            agent,
        }
    }

    fn warp() -> CodingAgentSpec {
        the_builtin_spec("warp", crate::WARP_SPEC_YAML)
    }

    #[test]
    fn a_host_only_spec_refuses_the_jail_naming_its_placements() {
        // When
        let refused = warp().require_placement(Placement::Jail);

        // Then
        assert_eq!(
            (
                refused.clone(),
                refused.map_err(|error| error.to_string()).unwrap_err()
            ),
            (
                Err(PlacementError::NotPlaced {
                    tool: "warp".to_string(),
                    placement: Placement::Jail,
                    placements: vec![Placement::Host],
                }),
                "warp cannot run in the jail: its spec's placements are [host]".to_string(),
            )
        );
    }

    #[test]
    fn a_host_only_spec_admits_the_host() {
        // When
        let admitted = warp().require_placement(Placement::Host);

        // Then
        assert_eq!(admitted, Ok(()));
    }

    #[test]
    fn a_spec_that_declares_no_placements_runs_on_the_host_and_in_a_jail() {
        // Given — claude.yaml declares no `placements`
        let claude = the_builtin_spec("claude", include_str!("../specs/claude.yaml"));

        // When
        let admitted = (
            claude.agent.placements.clone(),
            claude.require_placement(Placement::Host),
            claude.require_placement(Placement::Jail),
        );

        // Then
        assert_eq!(
            admitted,
            (vec![Placement::Host, Placement::Jail], Ok(()), Ok(()))
        );
    }
}

#[cfg(test)]
mod extension_validation_tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::ExtensionOwner;

    use super::CodingAgentExtension;

    /// The `coding_agent` section of the built-in OpenCode spec, as YAML.
    fn the_opencode_section() -> serde_yaml::Value {
        let document: serde_yaml::Value = serde_yaml::from_str(crate::OPENCODE_SPEC_YAML).unwrap();
        document["extensions"]["coding_agent"].clone()
    }

    fn validate(section: &serde_yaml::Value) -> Result<(), String> {
        CodingAgentExtension.validate("opencode", section)
    }

    #[test]
    fn the_builtin_opencode_section_validates() {
        // When
        let outcome = validate(&the_opencode_section());

        // Then
        assert_eq!(outcome, Ok(()));
    }

    #[test]
    fn a_models_adapter_the_build_does_not_hold_is_refused_naming_it() {
        // Given
        let mut section = the_opencode_section();
        section["models"]["adapter"] = serde_yaml::Value::String("no-such-adapter".to_string());

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "models.adapter no-such-adapter is not a launch adapter this build has".to_string()
            )
        );
    }

    #[test]
    fn a_models_list_command_without_its_adapter_is_refused() {
        // Given
        let mut section = the_opencode_section();
        section["models"]
            .as_mapping_mut()
            .unwrap()
            .remove("adapter");

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "models.list_command and models.adapter are declared together or not at all"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_resume_adapter_the_build_does_not_hold_is_refused_naming_it() {
        // Given
        let mut section = the_opencode_section();
        section["resume"] = serde_yaml::from_str("adapter: no-such-adapter").unwrap();

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "resume.adapter no-such-adapter is not a launch adapter this build has".to_string()
            )
        );
    }

    #[test]
    fn an_empty_placements_list_is_refused() {
        // Given
        let mut section = the_opencode_section();
        section["placements"] = serde_yaml::Value::Sequence(Vec::new());

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err("placements is empty: an agent runs on at least one of host, jail".to_string())
        );
    }
}
