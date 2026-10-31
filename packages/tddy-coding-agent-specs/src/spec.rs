//! The `coding_agent` extension: its schema, its owner, and the validated view of one agent.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tddy_actions::tool::{ArgvItem, ExtensionOwner, RunSection, ToolDef, Transport};
use thiserror::Error;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSlot {
    Flag(FlagSlot),
    Unsupported(Unsupported),
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
            ToolSlot::Unsupported(_) => None,
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
