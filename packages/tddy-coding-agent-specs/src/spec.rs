//! The `coding_agent` extension: its schema, its owner, and the validated view of one agent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tddy_actions::tool::{ArgvItem, ExtensionOwner, RunSection, ToolDef, Transport};
use thiserror::Error;

use crate::capability::Capability;
use crate::route::{ProviderKind, RouteShape, RouteSupport};

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
        // TODO(claude-host): implement — deserialize a `CodingAgentSection`, resolve the hooks adapter.
        // TODO(claude-headless): and refuse a `headless` section whose adapter is unregistered —
        // `crate::headless::validate_headless(section, &AdapterRegistry::builtin())`.
        // TODO(harness-clis): and refuse a `models.adapter` / `resume.adapter` that
        // `tddy_agent_backend::adapters::LaunchAdapters::builtin()` does not hold, a `models` with
        // one of `list_command`/`adapter` but not the other, and an empty `placements`.
        // TODO(acp-agents): and refuse an `acp.hooks_adapter` that
        // `tddy_agent_backend::adapters::AcpHooksAdapters::builtin()` does not hold, and a spec whose
        // `run.transport` is `acp` but which declares no `acp` section.
        let _ = (tool_id, section);
        todo!("claude-host: CodingAgentExtension::validate")
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
        // TODO(claude-host): implement.
        let _ = def;
        todo!("claude-host: CodingAgentSpec::of")
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
}

impl CodingAgentSpec {
    /// The capabilities the agent declares.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.agent.capabilities
    }

    /// Whether the agent declares `capability` — the question every agent gate asks.
    pub fn has(&self, capability: Capability) -> bool {
        // TODO(agent-registry): implement.
        let _ = capability;
        todo!("agent-registry: CodingAgentSpec::has")
    }

    /// What a provider route can do with the agent: `coding_agent.route.providers` and `.shape`, or
    /// no providers when the spec has no `route` section — an agent that cannot be routed.
    pub fn route_support(&self) -> RouteSupport {
        // TODO(agent-registry): implement.
        todo!("agent-registry: CodingAgentSpec::route_support")
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

impl CodingAgentSpec {
    /// The agent's ACP declaration, if it has one.
    pub fn acp(&self) -> Option<&AcpSection> {
        self.agent.acp.as_ref()
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
        // TODO(jail-agnostic): implement.
        let _ = name;
        todo!("jail-agnostic: CodingAgentSpec::variant_args")
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
        // TODO(harness-clis): implement.
        let _ = placement;
        todo!("harness-clis: CodingAgentSpec::require_placement")
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
    /// The variable the route's key is handed in (`TDDY_OPENCODE_PROVIDER_KEY`, `META_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// The providers a route can point the agent at (`#agent-tools 12/12`); empty for an agent that
    /// always runs on its own vendor. Read through [`CodingAgentSpec::route_support`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderKind>,
    /// How the route reaches the agent (`#agent-tools 12/12`): environment (the default),
    /// arguments, or a configuration file.
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
}

impl HeadlessSection {
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
    /// A file in `format`, passed by `flag <path>`.
    File { flag: String, format: McpFormat },
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

/// The models an agent offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsSection {
    pub default: String,
    pub curated: Vec<String>,
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
