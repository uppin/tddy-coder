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
mod session;
pub mod slots;
mod spec;

use std::path::Path;

use tddy_actions::tool::{ToolError, ToolRegistry, BUILTIN_SUPPORT_TOOLS, BUILTIN_TOOLS};

pub use crate::spec::acp::package_runner;
pub use crate::spec::acp::AcpCommandError;
pub use crate::spec::acp::AcpSection;
pub use crate::spec::argv::CuratedModel;
pub use crate::spec::argv::ModelsSection;
pub use crate::spec::headless_section::HeadlessPermissionPolicy;
pub use crate::spec::headless_section::HeadlessSection;
pub use crate::spec::route_slot::routed_agent;
pub use crate::spec::route_slot::Placement;
pub use crate::spec::route_slot::PlacementError;
pub use crate::spec::route_slot::ResumeSlot;
pub use crate::spec::route_slot::RouteSlot;
pub use crate::spec::slot_shapes::FlagSlot;
pub use crate::spec::slot_shapes::HooksSlot;
pub use crate::spec::slot_shapes::InstructionsSlot;
pub use crate::spec::slot_shapes::McpFormat;
pub use crate::spec::slot_shapes::McpSlot;
pub use crate::spec::slot_shapes::ToolSlot;
pub use crate::spec::slot_shapes::Unsupported;
pub use agent_env_base::{
    AgentEnvBase, AgentEnvBaseError, AGENT_ENV_BASE_EMBEDDED_PATH, AGENT_ENV_BASE_OVERRIDE,
    AGENT_ENV_BASE_YAML,
};
pub use capability::Capability;
pub use registry::{CodingAgentRegistry, RegistryError};
pub use route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent};
pub use session::{
    is_no_agent_session_type, DeclaredSessionType, LaunchKind, SessionExtension, SessionSection,
    NO_AGENT_SESSION_TYPE, SESSION_EXTENSION,
};
pub use spec::{
    AgentVariant, CodingAgentExtension, CodingAgentSection, CodingAgentSpec, SpecError,
    VariantError, CODING_AGENT_EXTENSION, DEFAULT_ANSWER_WAIT_SECS, HOST_AGENT_VARIANT,
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

/// `session-tools/tddy-coder.yaml`, embedded: the tool that declares the `tool` session type. It
/// lives apart from `specs/`, which holds only coding agents.
pub const TDDY_CODER_SPEC_YAML: &str = include_str!("../session-tools/tddy-coder.yaml");

/// The embedded tool files that declare a session type but are no coding agent: `(source path,
/// YAML)`. Kept apart from [`builtin_specs`], every one of which is a coding agent.
pub fn builtin_session_tools() -> Vec<(&'static str, &'static str)> {
    vec![(
        "packages/tddy-coding-agent-specs/session-tools/tddy-coder.yaml",
        TDDY_CODER_SPEC_YAML,
    )]
}

/// Every embedded tool file a host that launches sessions loads: `tddy-actions`' built-in and
/// support tools, [`builtin_specs`] and [`builtin_session_tools`].
pub(crate) fn embedded_tool_sources() -> Vec<(&'static str, &'static str)> {
    BUILTIN_TOOLS
        .iter()
        .chain(BUILTIN_SUPPORT_TOOLS)
        .copied()
        .chain(builtin_specs())
        .chain(builtin_session_tools())
        .collect()
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

/// The registry a host that launches coding agents uses: the [`embedded_tool_sources`], with
/// `<tddyhome>/tools` (`overrides_dir`) applied and every `extensions:` section validated by its
/// owner — [`CodingAgentExtension`] or [`SessionExtension`].
///
/// An override of a built-in that declares a session type must declare one too: dropping the
/// `session` section would silently take the built-in's session type off the host (and with it every
/// stored session of that type), so the override is refused, naming its file and the built-in's
/// values. Nothing is inherited from the built-in. A new tool that overrides no built-in may declare
/// none.
pub fn load_agent_registry(overrides_dir: &Path) -> Result<ToolRegistry, ToolError> {
    let builtin = ToolRegistry::from_embedded(&embedded_tool_sources())?;
    let loaded = ToolRegistry::load_with_owners(
        builtin.clone(),
        overrides_dir,
        &[&CodingAgentExtension, &SessionExtension],
    )?;
    refuse_overrides_that_drop_a_session_type(&builtin, &loaded)?;
    Ok(loaded)
}

/// [`load_agent_registry`]'s refusal of an override that drops its built-in's `session` section.
fn refuse_overrides_that_drop_a_session_type(
    builtin: &ToolRegistry,
    loaded: &ToolRegistry,
) -> Result<(), ToolError> {
    for id in builtin.ids() {
        let (Some(builtin_def), Some(loaded_def)) = (builtin.get(id), loaded.get(id)) else {
            continue;
        };
        let Some(declared) = builtin_def.extensions.get(SESSION_EXTENSION) else {
            continue;
        };
        if loaded.origin(id) == builtin.origin(id)
            || loaded_def.extensions.contains_key(SESSION_EXTENSION)
        {
            continue;
        }
        let declared: SessionSection =
            serde_yaml::from_value(declared.clone()).map_err(|e| ToolError::ExtensionRejected {
                file: builtin
                    .origin(id)
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
                owner: SESSION_EXTENSION.to_string(),
                message: e.to_string(),
            })?;
        return Err(ToolError::ExtensionRejected {
            file: loaded.origin(id).map(Path::to_path_buf).unwrap_or_default(),
            owner: SESSION_EXTENSION.to_string(),
            message: format!(
                "this file overrides the built-in {id}, which declares a session type, but declares \
                 none itself; add `extensions.session: {{ session_type: {}, launch: {}, label: \
                 {} }}` (the built-in's values) or another session type of its own",
                declared.session_type, declared.launch, declared.label
            ),
        });
    }
    Ok(())
}
