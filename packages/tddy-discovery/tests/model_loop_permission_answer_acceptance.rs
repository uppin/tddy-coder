//! A model-loop subagent never asks permission, so a resume carrying a permission answer is
//! refused before the model is called.
//!
//! PRD: docs/ft/coder/managed-codebase-subagents.md § Permission requests

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, PermissionAnswer, SubagentConfig, SubagentRegistry, TurnRequest,
};
use wiremock::MockServer;

fn a_model_loop_over(server: &MockServer) -> Box<dyn tddy_discovery::subagent::SubagentSession> {
    let def = SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: server.uri(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read],
        max_turns: 3,
        replaces: Vec::new(),
        usage_notes: None,
    };
    let codebase = CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async { String::new() })
        },
    );
    SubagentRegistry::from_defs(vec![def])
        .create("explorer", SubagentConfig::new(codebase))
        .expect("the def must resolve")
}

#[tokio::test]
async fn a_permission_answer_to_a_model_loop_is_refused_without_calling_the_model() {
    // Given
    let server = MockServer::start().await;
    let mut session = a_model_loop_over(&server);

    // When
    let refused = session
        .take_turn(
            TurnRequest::resuming().with_permission_answer(PermissionAnswer::Cancelled {
                request_id: "perm-1".to_string(),
            }),
        )
        .await
        .expect_err("a model loop has no permission request to answer");

    // Then
    assert!(
        refused.to_string().contains("perm-1"),
        "the refusal names the answer: {refused}"
    );
    let calls = server.received_requests().await.expect("requests recorded");
    assert!(calls.is_empty(), "the model was not called");
}
