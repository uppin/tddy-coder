//! Acceptance tests for the backend abstraction (OCP refactor).
//!
//! These tests define the expected behavior of the refactored CodingBackend trait,
//! InvokeRequest with goal id + hints, and InvokeResponse with optional session_id.

mod common;

use tddy_core::{CodingBackend, GoalId, MockBackend};

/// InvokeRequest carries goal id and hints (no compile-time Goal enum).
#[test]
fn invoke_request_has_goal_id_and_hints() {
    // When
    let req = common::stub_invoke_request("test", "plan");

    // Then
    assert_eq!(req.goal_id, GoalId::new("plan"));
    assert!(!req.hints.display_name.is_empty(), "goal hints must have a non-empty display name");
}

/// InvokeRequest can be built for every goal id defined by the TDD recipe.
#[test]
fn invoke_request_supports_all_tdd_goal_ids() {
    // Given
    let recipe = common::tdd_recipe();

    // When / Then
    for gid in recipe.goal_ids() {
        let req = common::stub_invoke_request("test", gid.as_str());
        assert_eq!(req.goal_id, gid, "goal_id must match for goal {}", gid.as_str());
        assert_eq!(req.submit_key, recipe.submit_key(&gid), "submit_key must match for goal {}", gid.as_str());
    }
}

/// InvokeResponse.session_id is Option<String> for backends that may not return a session.
#[test]
fn invoke_response_session_id_is_option() {
    // Given — response without session
    let resp = tddy_testing_commons::builders::an_invoke_response().build();

    // Then
    assert_eq!(resp.session_id, None, "session_id must default to None");

    // Given — response with session
    let resp_with_session = tddy_testing_commons::builders::an_invoke_response()
        .with_session_id("session-123")
        .build();

    // Then
    assert_eq!(resp_with_session.session_id.as_deref(), Some("session-123"), "session_id must round-trip");
}

/// CodingBackend trait has name() method returning backend identifier.
#[test]
fn coding_backend_has_name_method() {
    // Given
    let backend = MockBackend::new();

    // When / Then
    assert_eq!(backend.name(), "mock", "MockBackend must identify itself as 'mock'");
}
