//! The `coding_agent` extension: its schema, its owner, and the validated view of one agent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tddy_actions::tool::{ExtensionOwner, RunSection, ToolDef};
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
    /// The flag each pre-approved tool is passed with.
    pub tool_allow: FlagSlot,
    /// The flag each withdrawn tool is passed with.
    pub tool_deny: FlagSlot,
    /// The flag naming the MCP tool the agent asks before running anything not pre-approved.
    pub permission_prompt: FlagSlot,
    pub instructions: InstructionsSlot,
    pub hooks: HooksSlot,
    pub models: ModelsSection,
    /// What context sync copies from a codebase into the agent's working directory.
    #[serde(default)]
    pub context_globs: Vec<String>,
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
