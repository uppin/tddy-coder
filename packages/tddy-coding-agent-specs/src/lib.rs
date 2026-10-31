//! Coding agents as declared tools (`#agent-tools`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).
//!
//! An agent is a `tddy_actions::tool::ToolDef`: its `run:` section is the argv template, its
//! `deps` name the tools it calls back through (`tddy-tools`), and its `extensions.coding_agent`
//! section — owned here, by [`CodingAgentExtension`] — declares the tddy **mapping slots**. A
//! launcher renders the template with `tddy_actions::tool::render_argv` and the slots with
//! [`slots::render`]; nothing in that path hard-codes an agent.

pub mod headless;
pub mod slots;
mod spec;

use std::path::Path;

use tddy_actions::tool::{ToolError, ToolRegistry};

pub use spec::{
    CodingAgentExtension, CodingAgentSection, CodingAgentSpec, FlagSlot, HeadlessSection,
    HooksSlot, InstructionsSlot, McpFormat, McpSlot, ModelsSection, Placement, PlacementError,
    ResumeSlot, RouteSlot, SpecError, ToolSlot, Unsupported, CODING_AGENT_EXTENSION,
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
    ]
}

/// The id of the agent spec a CLI session type runs (`claude-cli` → `claude`, `cursor-cli` →
/// `cursor`, `opencode-cli` → `opencode`, `muse-cli` → `muse`, `warp-cli` → `warp`); `None` for a
/// session type no spec stands for — a tool or workspace session (`#agent-tools 8/12`).
///
/// The one place a session type meets a spec id: the launchers and harness profiles resolve
/// through it instead of the `CliHarness` table.
pub fn spec_for_session_type(session_type: &str) -> Option<&'static str> {
    // TODO(harness-clis): implement.
    let _ = session_type;
    todo!("harness-clis: spec_for_session_type")
}

/// The registry a host that launches coding agents uses: `tddy-actions`' built-in and support
/// tools plus [`builtin_specs`], with `<tddyhome>/tools` (`overrides_dir`) applied and every
/// `extensions:` section validated by [`CodingAgentExtension`].
pub fn load_agent_registry(overrides_dir: &Path) -> Result<ToolRegistry, ToolError> {
    // TODO(claude-host): implement — `ToolRegistry::from_embedded(BUILTIN_TOOLS ++
    // BUILTIN_SUPPORT_TOOLS ++ builtin_specs())`, then `load_with_owners(…, &[&CodingAgentExtension])`.
    let _ = overrides_dir;
    todo!("claude-host: load_agent_registry")
}

#[cfg(test)]
mod spec_for_session_type_tests {
    use pretty_assertions::assert_eq;

    use super::spec_for_session_type;

    #[test]
    fn every_cli_session_type_names_its_agent_spec() {
        // When
        let resolved: Vec<Option<&str>> = [
            "claude-cli",
            "cursor-cli",
            "opencode-cli",
            "muse-cli",
            "warp-cli",
        ]
        .into_iter()
        .map(spec_for_session_type)
        .collect();

        // Then
        assert_eq!(
            resolved,
            vec![
                Some("claude"),
                Some("cursor"),
                Some("opencode"),
                Some("muse"),
                Some("warp"),
            ]
        );
    }

    #[test]
    fn a_session_type_no_spec_stands_for_is_none_while_a_cli_one_resolves() {
        // When
        let resolved = (
            spec_for_session_type("tool"),
            spec_for_session_type("workspace"),
            spec_for_session_type("opencode"),
            spec_for_session_type("opencode-cli"),
        );

        // Then — `opencode` is a tool-session agent, not a session type
        assert_eq!(resolved, (None, None, None, Some("opencode")));
    }
}
