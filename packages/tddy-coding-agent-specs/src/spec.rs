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
}

impl CodingAgentSpec {
    /// The agent's headless declaration, if it has one.
    pub fn headless(&self) -> Option<&HeadlessSection> {
        self.agent.headless.as_ref()
    }
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
}

/// The shape of an MCP registration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpFormat {
    /// `{"mcpServers": {"<name>": {"command", "args", "env"?}}}`.
    ClaudeMcpJson,
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
}

/// Which Rust hooks adapter writes the agent's lifecycle-hooks configuration. The hook protocol is
/// not declarable; a file can only name an adapter (`claude-settings-local`).
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
