//! An OpenCode session is served OpenCode's context files: `AGENTS.md`, `opencode.json` and the
//! `.opencode/` directory, on top of the shared base.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md

use tddy_core::backend::context_globs_for_agent;
use tddy_session_files::context_files::context_agent_for_session_type;

#[test]
fn an_opencode_session_is_served_opencodes_context_files() {
    // When
    let agent = context_agent_for_session_type("opencode-cli");
    let globs = context_globs_for_agent(agent);

    // Then
    assert_eq!(agent, "opencode");
    for expected in ["AGENTS.md", "opencode.json", ".opencode/**"] {
        assert!(
            globs.contains(&expected),
            "{expected} in {globs:?}"
        );
    }
}

#[test]
fn claude_cli_and_cursor_cli_keep_their_context_agents() {
    // Then
    assert_eq!(context_agent_for_session_type("claude-cli"), "claude");
    assert_eq!(context_agent_for_session_type("cursor-cli"), "cursor");
    assert_eq!(context_agent_for_session_type("workspace"), "");
}
