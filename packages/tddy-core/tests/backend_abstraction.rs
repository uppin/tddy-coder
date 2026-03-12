//! Acceptance tests for the backend abstraction (OCP refactor).
//!
//! These tests define the expected behavior of the refactored CodingBackend trait,
//! InvokeRequest with Goal enum, and InvokeResponse with optional session_id.

use tddy_core::{CodingBackend, Goal, InvokeRequest, InvokeResponse, MockBackend};

/// InvokeRequest contains only generic fields; no Claude-specific permission_mode or allowed_tools.
#[test]
fn invoke_request_has_goal_not_permission_mode() {
    let req = InvokeRequest {
        prompt: "test".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session: None,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
    };
    assert!(matches!(req.goal, Goal::Plan));
}

/// InvokeRequest supports all Goal variants.
#[test]
fn invoke_request_supports_all_goal_variants() {
    for goal in [Goal::Plan, Goal::AcceptanceTests, Goal::Red, Goal::Green] {
        let req = InvokeRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal,
            model: None,
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
        };
        assert_eq!(
            std::mem::discriminant(&req.goal),
            std::mem::discriminant(&goal)
        );
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
