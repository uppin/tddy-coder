//! Acceptance tests for the backend abstraction (OCP refactor).
//!
//! These tests define the expected behavior of the refactored CodingBackend trait,
//! InvokeRequest with goal id + hints, and InvokeResponse with optional session_id.

mod common;

use tddy_core::{CodingBackend, GoalId, InvokeResponse, MockBackend};

/// InvokeRequest carries goal id and hints (no compile-time Goal enum).
#[test]
fn invoke_request_has_goal_id_and_hints() {
    let req = common::stub_invoke_request("test", "plan");
    assert_eq!(req.goal_id, GoalId::new("plan"));
    assert!(!req.hints.display_name.is_empty());
}

/// InvokeRequest can be built for every goal id defined by the TDD recipe.
#[test]
fn invoke_request_supports_all_tdd_goal_ids() {
    let recipe = common::tdd_recipe();
    for gid in recipe.goal_ids() {
        let req = common::stub_invoke_request("test", gid.as_str());
        assert_eq!(req.goal_id, gid);
        assert_eq!(req.submit_key, recipe.submit_key(&gid));
    }
}

/// InvokeResponse.session_id is Option<String> for backends that may not return a session.
#[test]
fn invoke_response_session_id_is_option() {
    let resp = InvokeResponse {
        output: "out".to_string(),
        exit_code: 0,
        session_id: None,
        questions: vec![],
        raw_stream: None,
        stderr: None,
    };
    assert_eq!(resp.session_id, None);

    let resp_with_session = InvokeResponse {
        output: "out".to_string(),
        exit_code: 0,
        session_id: Some("session-123".to_string()),
        questions: vec![],
        raw_stream: None,
        stderr: None,
    };
    assert_eq!(resp_with_session.session_id.as_deref(), Some("session-123"));
}

/// CodingBackend trait has name() method returning backend identifier.
#[test]
fn coding_backend_has_name_method() {
    let backend = MockBackend::new();
    assert_eq!(backend.name(), "mock");
}
