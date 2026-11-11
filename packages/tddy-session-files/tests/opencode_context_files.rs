//! A session is served its agent's spec row: the session type names the agent, and the
//! caller's registry supplies that agent's `context_globs`.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

use tddy_session_files::context_files::{
    context_agent_for_session, context_globs, SessionContextAgent,
};

#[test]
fn an_opencode_session_is_served_its_spec_row() {
    // Given an OpenCode session and the row its spec declares (opencode.yaml's
    // `context_globs`, supplied here as the caller would read it from the registry)
    let meta = a_metadata("opencode-cli");
    let row: Vec<String> = ["AGENTS.md", "opencode.json", ".opencode/**"]
        .iter()
        .map(ToString::to_string)
        .collect();

    // When
    let agent = context_agent_for_session(&meta);
    let globs = context_globs(Some(&row));

    // Then
    assert_eq!(
        agent,
        SessionContextAgent::Own(Some("opencode-cli".to_string()))
    );
    for expected in ["AGENTS.md", "opencode.json", ".opencode/**"] {
        assert!(
            globs.contains(&expected.to_string()),
            "{expected} in {globs:?}"
        );
    }
}

#[test]
fn cli_session_types_name_their_agents_and_workspace_names_none() {
    // Given
    let meta_for = a_metadata;

    // Then
    assert_eq!(
        context_agent_for_session(&meta_for("claude-cli")),
        SessionContextAgent::Own(Some("claude-cli".to_string()))
    );
    assert_eq!(
        context_agent_for_session(&meta_for("cursor-cli")),
        SessionContextAgent::Own(Some("cursor-cli".to_string()))
    );
    assert_eq!(
        context_agent_for_session(&meta_for("workspace")),
        SessionContextAgent::Own(None)
    );
}

/// A session record carrying only `session_type`, as its daemon persisted it.
fn a_metadata(session_type: &str) -> tddy_core::SessionMetadata {
    serde_json::from_value(serde_json::json!({
        "session_id": "s1",
        "project_id": "p1",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "status": "running",
        "session_type": session_type,
    }))
    .expect("a session record deserializes")
}
