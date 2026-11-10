//! Coding agents as declared tools (`#agent-tools`).
//!
//! Spec: `docs/ft/daemon/claude-cli-session.md` § Launch (`#agent-tools 2/12`).
//!
//! An agent is a `tddy_actions::tool::ToolDef`: its `run:` section is the argv template, its
//! `deps` name the tools it calls back through (`tddy-tools`), and its `extensions.coding_agent`
//! section — owned here, by [`CodingAgentExtension`] — declares the tddy **mapping slots**. A
//! launcher renders the template with `tddy_actions::tool::render_argv` and the slots with
//! [`slots::render`]; nothing in that path hard-codes an agent.

mod agent_env_base;
mod capability;
pub mod claude_tools;
pub mod headless;
mod registry;
mod route;
pub mod slots;
mod spec;

use std::path::Path;

use tddy_actions::tool::{ToolError, ToolRegistry, BUILTIN_SUPPORT_TOOLS, BUILTIN_TOOLS};

pub use crate::spec::headless_section::HeadlessPermissionPolicy;
pub use crate::spec::headless_section::HeadlessSection;
pub use agent_env_base::{
    AgentEnvBase, AgentEnvBaseError, AGENT_ENV_BASE_EMBEDDED_PATH, AGENT_ENV_BASE_OVERRIDE,
    AGENT_ENV_BASE_YAML,
};
pub use capability::Capability;
pub use registry::{CodingAgentRegistry, RegistryError, CLI_SESSION_TYPE_SUFFIX};
pub use route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent};
pub use spec::{
    package_runner, routed_agent, AcpCommandError, AcpSection, AgentVariant, CodingAgentExtension,
    CodingAgentSection, CodingAgentSpec, CuratedModel, FlagSlot, HooksSlot, InstructionsSlot,
    McpFormat, McpSlot, ModelsSection, Placement, PlacementError, ResumeSlot, RouteSlot, SpecError,
    ToolSlot, Unsupported, VariantError, CODING_AGENT_EXTENSION, DEFAULT_ANSWER_WAIT_SECS,
    HOST_AGENT_VARIANT,
};

/// `specs/cursor.yaml` (`#agent-tools 7/12`), embedded.
pub const CURSOR_SPEC_YAML: &str = include_str!("../specs/cursor.yaml");

/// `specs/opencode.yaml` (`#agent-tools 8/12`), embedded.
pub const OPENCODE_SPEC_YAML: &str = include_str!("../specs/opencode.yaml");

/// `specs/muse.yaml` (`#agent-tools 8/12`), embedded.
pub const MUSE_SPEC_YAML: &str = include_str!("../specs/muse.yaml");

/// `specs/warp.yaml` (`#agent-tools 8/12`), embedded.
pub const WARP_SPEC_YAML: &str = include_str!("../specs/warp.yaml");

/// `specs/codex.yaml` (`#agent-tools 9/12`), embedded.
pub const CODEX_SPEC_YAML: &str = include_str!("../specs/codex.yaml");

/// `specs/oz.yaml` (`#agent-tools 9/12`), embedded.
pub const OZ_SPEC_YAML: &str = include_str!("../specs/oz.yaml");

/// `specs/claude-acp.yaml` (`#agent-tools 11/12`), embedded.
pub const CLAUDE_ACP_SPEC_YAML: &str = include_str!("../specs/claude-acp.yaml");

/// `specs/codex-acp.yaml` (`#agent-tools 11/12`), embedded.
pub const CODEX_ACP_SPEC_YAML: &str = include_str!("../specs/codex-acp.yaml");

/// The embedded agent specs: `(source path, YAML)`.
pub fn builtin_specs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "packages/tddy-coding-agent-specs/specs/claude.yaml",
            include_str!("../specs/claude.yaml"),
        ),
        (
            "packages/tddy-coding-agent-specs/specs/cursor.yaml",
            CURSOR_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/opencode.yaml",
            OPENCODE_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/muse.yaml",
            MUSE_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/warp.yaml",
            WARP_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/codex.yaml",
            CODEX_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/oz.yaml",
            OZ_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/claude-acp.yaml",
            CLAUDE_ACP_SPEC_YAML,
        ),
        (
            "packages/tddy-coding-agent-specs/specs/codex-acp.yaml",
            CODEX_ACP_SPEC_YAML,
        ),
    ]
}

/// The built-in spec of agent `id`, as embedded — without the `<tddyhome>/tools` overrides
/// [`load_agent_registry`] applies, and without resolving its binary.
///
/// For a caller that renders one agent's slots and has no tool registry to hand. No production
/// code is such a caller: a split session reads the host's registry, and a caller that launches the
/// agent loads the registry too. What still calls this is test fixtures.
pub fn embedded_spec(id: &str) -> Result<CodingAgentSpec, SpecError> {
    let (path, yaml) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&format!("/{id}.yaml")))
        .ok_or_else(|| SpecError::NotACodingAgent {
            tool: id.to_string(),
        })?;
    let def = tddy_actions::tool::ToolDef::parse(Path::new(path), yaml).map_err(|e| {
        SpecError::Invalid {
            tool: id.to_string(),
            message: e.to_string(),
        }
    })?;
    CodingAgentSpec::of(&def)
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
