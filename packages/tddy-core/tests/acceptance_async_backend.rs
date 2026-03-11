//! Acceptance tests for Milestone 2: async CodingBackend.
//!
//! Verifies that CodingBackend::invoke is async and can be awaited.

use tddy_core::{CodingBackend, Goal, InvokeRequest, MockBackend};

#[tokio::test]
async fn mock_backend_invoke_is_async() {
    let backend = MockBackend::new();
    backend.push_ok("test output");

    let req = InvokeRequest {
        prompt: "test".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
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

    let result = backend.invoke(req).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.output, "test output");
    assert_eq!(resp.exit_code, 0);
}
