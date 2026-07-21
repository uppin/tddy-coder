//! Unit tests for `metadata_publisher` — `session_metadata_json` shape, `merge_session_metadata`
//! preserving sibling keys, and the `apply_session_metadata_event` tap mapping.

use super::{
    a_default_session_metadata, apply_session_metadata_event, merge_session_metadata,
    session_metadata_json, SessionMetadata,
};
use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_core::{AppMode, ModeChangedDetails, PresenterEvent};

fn a_session_metadata() -> SessionMetadata {
    SessionMetadata {
        goal: "acceptance-tests".to_string(),
        state: "Red".to_string(),
        agent: "claude".to_string(),
        model: "sonnet-4".to_string(),
        activity_status: String::new(),
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/feature".to_string(),
        elapsed_display: "3m".to_string(),
        pending_elicitation: false,
    }
}

#[test]
fn session_metadata_json_produces_a_document_with_the_session_block_and_expected_fields() {
    // Given
    let meta = a_session_metadata();

    // When
    let json = session_metadata_json(&meta);

    // Then — the document is JSON with a `session` object carrying every field
    let v: Value = serde_json::from_str(&json).expect("session_metadata_json must emit valid JSON");
    let session = v
        .get("session")
        .expect("session_metadata_json must wrap fields in a `session` object");
    assert_eq!(
        session.get("workflow_goal").and_then(|x| x.as_str()),
        Some("acceptance-tests"),
        "workflow_goal must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("workflow_state").and_then(|x| x.as_str()),
        Some("Red"),
        "workflow_state must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("agent").and_then(|x| x.as_str()),
        Some("claude"),
        "agent must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("model").and_then(|x| x.as_str()),
        Some("sonnet-4"),
        "model must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("recipe").and_then(|x| x.as_str()),
        Some("tdd"),
        "recipe must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("repo_path").and_then(|x| x.as_str()),
        Some("/home/dev/feature"),
        "repo_path must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("elapsed_display").and_then(|x| x.as_str()),
        Some("3m"),
        "elapsed_display must be serialized; got: {json}"
    );
    assert_eq!(
        session.get("pending_elicitation").and_then(|x| x.as_bool()),
        Some(false),
        "pending_elicitation must be serialized; got: {json}"
    );
}

#[test]
fn merge_session_metadata_preserves_owned_project_count_and_codex_oauth_from_baseline() {
    // Given — a baseline advertising owned_project_count + codex_oauth
    let baseline = serde_json::json!({
        "owned_project_count": 3,
        "codex_oauth": { "username": "octocat" },
    })
    .to_string();
    let session_json = session_metadata_json(&a_session_metadata());

    // When
    let merged = merge_session_metadata(&baseline, &session_json);

    // Then — the merge keeps the sibling keys AND the new `session` block
    let v: Value = serde_json::from_str(&merged).expect("merge must emit valid JSON");
    assert_eq!(
        v.get("owned_project_count").and_then(|x| x.as_u64()),
        Some(3),
        "merge must preserve owned_project_count; got: {merged}"
    );
    assert_eq!(
        v.get("codex_oauth")
            .and_then(|x| x.get("username"))
            .and_then(|x| x.as_str()),
        Some("octocat"),
        "merge must preserve codex_oauth; got: {merged}"
    );
    assert!(
        v.get("session").is_some(),
        "merge must include the new `session` block; got: {merged}"
    );
}

fn a_seed() -> super::SessionMetadataSeed {
    super::SessionMetadataSeed {
        agent: "claude".to_string(),
        model: "sonnet-4".to_string(),
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/feature".to_string(),
    }
}

#[test]
fn a_default_session_metadatas_static_fields_and_empties_workflow_fields() {
    // Given
    let seed = a_seed();

    // When
    let meta = a_default_session_metadata(&seed);

    // Then — static fields are seeded; workflow-derived fields start empty / false
    assert_eq!(meta.agent, "claude");
    assert_eq!(meta.model, "sonnet-4");
    assert_eq!(meta.recipe, "tdd");
    assert_eq!(meta.repo_path, "/home/dev/feature");
    assert_eq!(meta.goal, "");
    assert_eq!(meta.state, "");
    assert!(!meta.pending_elicitation);
}

#[test]
fn apply_backend_selected_updates_agent_and_model() {
    // Given
    let mut meta = a_default_session_metadata(&a_seed());

    // When
    let updated = apply_session_metadata_event(
        &meta,
        &PresenterEvent::BackendSelected {
            agent: "codex".to_string(),
            model: "gpt-5".to_string(),
        },
    );

    // Then
    let next = updated.expect("BackendSelected must change the snapshot");
    assert_eq!(next.agent, "codex");
    assert_eq!(next.model, "gpt-5");
    meta = next;

    // And — a no-op BackendSelected (same values) returns None
    let again = apply_session_metadata_event(
        &meta,
        &PresenterEvent::BackendSelected {
            agent: "codex".to_string(),
            model: "gpt-5".to_string(),
        },
    );
    assert!(again.is_none(), "a no-op event must not re-publish");
}

#[test]
fn apply_goal_started_sets_the_workflow_goal() {
    // Given
    let meta = a_default_session_metadata(&a_seed());

    // When
    let updated = apply_session_metadata_event(&meta, &PresenterEvent::GoalStarted("plan".into()));

    // Then
    let next = updated.expect("GoalStarted must change the snapshot");
    assert_eq!(next.goal, "plan");
}

#[test]
fn apply_state_changed_sets_the_workflow_state() {
    // Given
    let meta = a_default_session_metadata(&a_seed());

    // When
    let updated = apply_session_metadata_event(
        &meta,
        &PresenterEvent::StateChanged {
            from: "Planning".into(),
            to: "Red".into(),
        },
    );

    // Then
    let next = updated.expect("StateChanged must change the snapshot");
    assert_eq!(next.state, "Red");
}

#[test]
fn apply_mode_changed_sets_pending_elicitation_for_an_elicitation_mode() {
    // Given
    let meta = a_default_session_metadata(&a_seed());

    // When — a free-form text input (an elicitation mode)
    let updated = apply_session_metadata_event(
        &meta,
        &PresenterEvent::ModeChanged(ModeChangedDetails {
            mode: AppMode::TextInput {
                prompt: "Describe the feature".into(),
            },
            plan_refinement_pending: false,
            skills_project_root: None,
            awaiting_open_answer: false,
        }),
    );

    // Then
    let next = updated.expect("a TextInput mode must change the snapshot");
    assert!(
        next.pending_elicitation,
        "pending_elicitation must be true while an elicitation is shown"
    );
}

#[test]
fn apply_mode_changed_seeds_state_from_running_only_when_state_is_empty() {
    // Given — a fresh snapshot (no StateChanged yet)
    let meta = a_default_session_metadata(&a_seed());

    // When — Running mode
    let updated = apply_session_metadata_event(
        &meta,
        &PresenterEvent::ModeChanged(ModeChangedDetails {
            mode: AppMode::Running,
            plan_refinement_pending: false,
            skills_project_root: None,
            awaiting_open_answer: false,
        }),
    );

    // Then — state is seeded to "running" as a fallback
    let next = updated.expect("Running mode must seed state when empty");
    assert_eq!(next.state, "running");

    // And — once StateChanged sets a real state, a later ModeChanged(Running) does not override it
    let after_state = apply_session_metadata_event(
        &next,
        &PresenterEvent::StateChanged {
            from: "running".into(),
            to: "Red".into(),
        },
    )
    .expect("StateChanged must apply");
    let no_override = apply_session_metadata_event(
        &after_state,
        &PresenterEvent::ModeChanged(ModeChangedDetails {
            mode: AppMode::Running,
            plan_refinement_pending: false,
            skills_project_root: None,
            awaiting_open_answer: false,
        }),
    );
    assert!(
        no_override.is_none() || no_override.unwrap().state == "Red",
        "ModeChanged must not override an authoritative StateChanged value"
    );
}

#[test]
fn apply_workflow_complete_sets_done_on_success_and_error_on_failure() {
    // Given
    let meta = a_default_session_metadata(&a_seed());

    // When — successful completion
    let ok = apply_session_metadata_event(
        &meta,
        &PresenterEvent::WorkflowComplete(Ok(tddy_core::presenter::WorkflowCompletePayload {
            summary: "done".into(),
            session_dir: None,
        })),
    );

    // Then
    assert_eq!(
        ok.expect("WorkflowComplete(Ok) must change state").state,
        "done"
    );

    // And — failed completion
    let err =
        apply_session_metadata_event(&meta, &PresenterEvent::WorkflowComplete(Err("boom".into())));
    assert_eq!(
        err.expect("WorkflowComplete(Err) must change state").state,
        "error"
    );
}

#[test]
fn apply_an_irrelevant_event_returns_none() {
    // Given
    let meta = a_default_session_metadata(&a_seed());

    // When — an event the tap does not map (e.g. AgentOutput)
    let updated = apply_session_metadata_event(&meta, &PresenterEvent::AgentOutput("hi".into()));

    // Then — no change, nothing to publish
    assert!(updated.is_none());
}
