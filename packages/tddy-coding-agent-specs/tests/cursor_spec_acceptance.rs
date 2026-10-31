//! Acceptance: `specs/cursor.yaml` is a built-in coding agent whose `coding_agent` section declares
//! Cursor's slots — the placed `cursor-mcp-json` registration, the managed-workflow rules file and
//! the `cursor-hooks-json` adapter, with no tool flags (`unsupported`). Its jail sections are exercised by the jailed launch
//! (`tddy-session-lifecycle` `cursor_cli_from_spec_acceptance`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md` (`#agent-tools 7/12`).

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{
    load_agent_registry, CodingAgentSpec, HooksSlot, McpFormat, McpSlot, ToolSlot, Unsupported,
};

#[test]
fn cursor_yamls_coding_agent_section_declares_the_placed_mcp_json_the_rules_file_the_hooks_adapter_and_no_tool_flags(
) {
    // Given — a fresh `<tddyhome>` with no overrides
    let home = tempfile::tempdir().unwrap();
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");

    // When
    let section = CodingAgentSpec::of(registry.get("cursor").expect("cursor is built in"))
        .expect("cursor is a coding agent")
        .agent;

    // Then — Cursor has no allow, deny or permission-prompt flag
    let unsupported = ToolSlot::Unsupported(Unsupported::Unsupported);
    assert_eq!(
        (
            section.tool_allow,
            section.tool_deny,
            section.permission_prompt,
        ),
        (unsupported.clone(), unsupported.clone(), unsupported)
    );
    // Then
    assert_eq!(
        (
            section.mcp,
            section.instructions.files,
            section.instructions.rules_file,
            section.instructions.append_system_prompt,
            section.hooks,
        ),
        (
            McpSlot::Placed {
                path: "{home}/.cursor/mcp.json".to_string(),
                format: McpFormat::CursorMcpJson,
            },
            Vec::<PathBuf>::new(),
            Some(PathBuf::from(".cursor/rules/tddy-managed-workflow.mdc")),
            None,
            HooksSlot {
                adapter: "cursor-hooks-json".to_string(),
            },
        )
    );
}

#[test]
fn the_builtin_registry_holds_cursor_beside_claude_as_a_coding_agent() {
    // Given — a fresh `<tddyhome>` with no overrides
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");

    // Then
    let ids: Vec<String> = ["claude", "cursor"]
        .iter()
        .filter_map(|id| registry.get(id))
        .filter_map(|def| CodingAgentSpec::of(def).ok())
        .map(|spec| spec.id)
        .collect();
    assert_eq!(ids, vec!["claude".to_string(), "cursor".to_string()]);
}
