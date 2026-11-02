//! Coding agents as declared tools (`#agent-tools`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).
//!
//! An agent is a `tddy_actions::tool::ToolDef`: its `run:` section is the argv template, its
//! `deps` name the tools it calls back through (`tddy-tools`), and its `extensions.coding_agent`
//! section — owned here, by [`CodingAgentExtension`] — declares the tddy **mapping slots**. A
//! launcher renders the template with `tddy_actions::tool::render_argv` and the slots with
//! [`slots::render`]; nothing in that path hard-codes an agent.

pub mod slots;
mod spec;

use std::path::Path;

use tddy_actions::tool::{ToolError, ToolRegistry, BUILTIN_SUPPORT_TOOLS, BUILTIN_TOOLS};

pub use spec::{
    CodingAgentExtension, CodingAgentSection, CodingAgentSpec, FlagSlot, HooksSlot,
    InstructionsSlot, McpFormat, McpSlot, ModelsSection, SpecError, ToolSlot, Unsupported,
    CODING_AGENT_EXTENSION,
};

/// The embedded agent specs: `(source path, YAML)`.
pub fn builtin_specs() -> Vec<(&'static str, &'static str)> {
    vec![(
        "packages/tddy-coding-agent-specs/specs/claude.yaml",
        include_str!("../specs/claude.yaml"),
    )]
}

/// The registry a host that launches coding agents uses: `tddy-actions`' built-in and support
/// tools plus [`builtin_specs`], with `<tddyhome>/tools` (`overrides_dir`) applied and every
/// `extensions:` section validated by [`CodingAgentExtension`].
pub fn load_agent_registry(overrides_dir: &Path) -> Result<ToolRegistry, ToolError> {
    let specs = builtin_specs();
    let sources: Vec<(&str, &str)> = BUILTIN_TOOLS
        .iter()
        .chain(BUILTIN_SUPPORT_TOOLS)
        .chain(&specs)
        .copied()
        .collect();
    let builtin = ToolRegistry::from_embedded(&sources)?;
    ToolRegistry::load_with_owners(builtin, overrides_dir, &[&CodingAgentExtension])
}
