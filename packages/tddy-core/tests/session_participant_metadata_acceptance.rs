//! Acceptance: the `session` block a session's LiveKit participant publishes carries its **stack
//! association**, and one crate owns its shape.
//!
//! Cross-host session visibility in the web is built entirely on presence — `ListSessions` does not
//! fan out — so a participant's metadata is the only place the PR-Stack view can learn which planned
//! PR a session on another host is working. The block therefore names the session, the orchestrator
//! that spawned it, the planned node it materializes and the branch it created (D37).
//!
//! The shape lives here rather than in `tddy-coder` because **two** publishers now emit it: the coder
//! for the sessions it runs, and the daemon for a `claude-cli` session, whose LiveKit bridge
//! previously published nothing at all. Participant metadata is merged **shallowly**, so a partial
//! `session` object from either publisher would erase the other's keys rather than merge with them —
//! which makes a single owned shape a correctness requirement, not tidiness.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37).

use serde_json::Value;
use tddy_core::session_participant_metadata::{session_metadata_json, SessionParticipantMetadata};

/// A stack child's block, fully populated — every field a scenario might read.
fn a_stack_child_block() -> SessionParticipantMetadata {
    SessionParticipantMetadata {
        goal: "acceptance-tests".to_string(),
        state: "Red".to_string(),
        agent: "claude".to_string(),
        model: "sonnet-4".to_string(),
        activity_status: "Running".to_string(),
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/pr-stack-project".to_string(),
        elapsed_display: "3m".to_string(),
        pending_elicitation: false,
        session_id: "dddddddd-0000-4000-8000-000000000004".to_string(),
        orchestrator_session_id: "pr-stack-session-1".to_string(),
        stack_node_id: "n1".to_string(),
        branch: "feature/attach-docs/attach-store".to_string(),
    }
}

/// The `session` object of a published document.
fn session_object_of(json: &str) -> Value {
    let v: Value = serde_json::from_str(json).expect("the published metadata must be valid JSON");
    v.get("session")
        .cloned()
        .expect("every publisher wraps its fields in a `session` object")
}

fn string_field(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_else(|| panic!("the `session` block must carry `{key}`"))
        .to_string()
}

#[test]
fn the_session_block_names_the_session_it_describes() {
    // Given — the web recovers the session id from the participant identity, but a block that does
    // not name itself cannot be checked against it
    let meta = a_stack_child_block();

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then
    assert_eq!(
        string_field(&session, "session_id"),
        "dddddddd-0000-4000-8000-000000000004"
    );
}

#[test]
fn the_session_block_names_the_orchestrator_that_spawned_the_session() {
    // Given
    let meta = a_stack_child_block();

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then — without it a cross-host stack child is an unrelated flat row in the drawer
    assert_eq!(
        string_field(&session, "orchestrator_session_id"),
        "pr-stack-session-1"
    );
}

#[test]
fn the_session_block_names_the_planned_node_the_session_materializes() {
    // Given
    let meta = a_stack_child_block();

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then — the exact identity: it survives a branch rename and it survives the host boundary
    assert_eq!(string_field(&session, "stack_node_id"), "n1");
}

#[test]
fn the_session_block_names_the_branch_the_session_created() {
    // Given
    let meta = a_stack_child_block();

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then
    assert_eq!(
        string_field(&session, "branch"),
        "feature/attach-docs/attach-store"
    );
}

#[test]
fn the_session_block_still_carries_every_workflow_field_it_did_before() {
    // Given — the association is additive; a drawer row reading goal/state/agent/model must not lose
    // them because a stack child now publishes more
    let meta = a_stack_child_block();

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then
    assert_eq!(string_field(&session, "workflow_goal"), "acceptance-tests");
    assert_eq!(string_field(&session, "workflow_state"), "Red");
    assert_eq!(string_field(&session, "agent"), "claude");
    assert_eq!(string_field(&session, "model"), "sonnet-4");
    assert_eq!(string_field(&session, "activity_status"), "Running");
    assert_eq!(string_field(&session, "recipe"), "tdd");
    assert_eq!(
        string_field(&session, "repo_path"),
        "/home/dev/pr-stack-project"
    );
    assert_eq!(string_field(&session, "elapsed_display"), "3m");
    assert_eq!(
        session.get("pending_elicitation").and_then(|x| x.as_bool()),
        Some(false),
        "pending_elicitation must be serialized as a boolean"
    );
}

#[test]
fn a_session_that_is_nobodys_stack_child_publishes_an_empty_association() {
    // Given — an ordinary session: the keys are present and empty rather than absent, so a reader
    // never has to tell "no association" from "an older publisher that omitted the key"
    let meta = SessionParticipantMetadata {
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/feature".to_string(),
        session_id: "aaaaaaaa-0000-4000-8000-000000000001".to_string(),
        ..SessionParticipantMetadata::default()
    };

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then
    assert_eq!(string_field(&session, "orchestrator_session_id"), "");
    assert_eq!(string_field(&session, "stack_node_id"), "");
    assert_eq!(string_field(&session, "branch"), "");
}

#[test]
fn the_daemon_can_publish_the_association_before_any_workflow_event_has_landed() {
    // Given — a claude-cli session, whose block the daemon writes once at spawn: it knows the
    // identity, the association and the static fields, and nothing about the agent's progress
    let meta = SessionParticipantMetadata {
        agent: "claude".to_string(),
        model: "sonnet-4".to_string(),
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/pr-stack-project".to_string(),
        session_id: "dddddddd-0000-4000-8000-000000000004".to_string(),
        orchestrator_session_id: "pr-stack-session-1".to_string(),
        stack_node_id: "n1".to_string(),
        branch: "feature/attach-docs/attach-store".to_string(),
        ..SessionParticipantMetadata::default()
    };

    // When
    let session = session_object_of(&session_metadata_json(&meta));

    // Then — a partial `session` object would erase the other publisher's keys on the shallow merge,
    // so every key is emitted, empty where the publisher has nothing to say
    assert_eq!(string_field(&session, "stack_node_id"), "n1");
    assert_eq!(string_field(&session, "workflow_goal"), "");
    assert_eq!(string_field(&session, "workflow_state"), "");
    assert_eq!(string_field(&session, "activity_status"), "");
}
