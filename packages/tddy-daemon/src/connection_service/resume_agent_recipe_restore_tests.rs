use super::resume_agent_and_recipe;
use tddy_core::SessionMetadata;

fn metadata_from_yaml(yaml: &str) -> SessionMetadata {
    serde_yaml::from_str(yaml)
        .expect("test metadata YAML must deserialize into SessionMetadata")
}

#[test]
fn resume_restores_the_sessions_persisted_agent_and_recipe() {
    // Given a persisted cursor / pr-stack session
    let metadata = metadata_from_yaml(
        r#"session_id: 019f243a-8e31-7203-81dd-53f5ef8b9352
project_id: proj-prstack
created_at: "2026-07-02T19:07:25Z"
updated_at: "2026-07-02T19:07:25Z"
status: active
agent: cursor
recipe: pr-stack
"#,
    );

    // When the daemon derives the spawn's agent and recipe for a resume
    let (agent, recipe) = resume_agent_and_recipe(&metadata);

    // Then the child is relaunched with the original agent and recipe, not the default claude
    assert_eq!(
        agent.as_deref(),
        Some("cursor"),
        "resume must restore the session's original agent, not fall back to default claude"
    );
    assert_eq!(
        recipe.as_deref(),
        Some("pr-stack"),
        "resume must restore the session's original recipe"
    );
}

#[test]
fn resume_of_a_legacy_session_without_persisted_agent_yields_none() {
    // Given a legacy session that predates agent/recipe persistence
    let metadata = metadata_from_yaml(
        r#"session_id: legacy-sess
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
"#,
    );

    // When the daemon derives the spawn's agent and recipe for a resume
    let (agent, recipe) = resume_agent_and_recipe(&metadata);

    // Then there is nothing to restore (tddy-coder applies its own resolution downstream)
    assert!(
        agent.is_none(),
        "legacy session has no persisted agent to restore"
    );
    assert!(
        recipe.is_none(),
        "legacy session has no persisted recipe to restore"
    );
}
