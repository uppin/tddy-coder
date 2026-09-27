//! Integration tests: a subagent turn goes through the provider queue before it reaches the
//! model.
//!
//! `provider_queue.rs` pins the gate and `subagent_provider_queue_visibility.rs` pins what
//! a caller is told. Neither of them proves the turn loop actually *uses* the gate — a queue built
//! and never wired in would pass both and change nothing about session 01a0e200, where the second
//! request went straight to Ollama's socket and waited there for 38 minutes.
//!
//! So these assert against the model endpoint itself: while the slot is held, the provider must
//! have received nothing at all.
//!
//! The provider a turn queues on is the def's `base_url`, which is what makes two agents on one
//! local Ollama contend and two agents on different endpoints not.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::pin::Pin;
use std::time::Duration;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{CodebaseAccess, SubagentConfig, SubagentRegistry, SubagentSession};
use tddy_discovery::subagent_runtime::ProviderQueue;
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const THE_GOAL: &str = "Where does the terminal switch to history on scroll?";
const SOMEONE_ELSE: &str = "response-fastcontext";

/// Long enough that an ungated turn would have reached the provider many times over — a wiremock
/// round trip is about a millisecond — so an endpoint that is still untouched at the end of it was
/// genuinely gated rather than merely unscheduled.
const LONG_ENOUGH_TO_REACH_AN_UNGATED_PROVIDER: Duration = Duration::from_millis(150);

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "gemma4:e4b-mlx".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns: 2,
        replaces: Vec::new(),
    }
}

fn a_codebase_that_answers() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({ "content": "ok", "truncated": false, "total_lines": 1 })
                    .to_string()
            })
        },
    )
}

/// A provider that answers any turn immediately, so what a test measures is admission and not
/// the model's own speed.
async fn a_model_that_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "terminalPresentation.ts" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;
    server
}

fn a_session_over(server: &MockServer, config: SubagentConfig) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", config)
        .expect("the def must resolve")
}

async fn requests_reaching(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock provider records its requests")
        .len()
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The property the incident needed: a busy provider is not sent a second request at all.
#[tokio::test]
async fn a_turn_does_not_reach_the_model_while_another_caller_holds_the_provider_slot() {
    // Given a session whose provider's only slot is already held
    let server = a_model_that_answers().await;
    let queue = ProviderQueue::new();
    let _held = queue
        .admit(&server.uri(), SOMEONE_ELSE)
        .await
        .expect("the other caller takes the slot first");
    let mut session = a_session_over(
        &server,
        SubagentConfig::new(a_codebase_that_answers()).with_provider_queue(queue.clone()),
    );

    // When it is prompted, and given long enough that an ungated turn would have been served
    let _ = tokio::time::timeout(
        LONG_ENOUGH_TO_REACH_AN_UNGATED_PROVIDER,
        session.prompt(THE_GOAL),
    )
    .await;

    // Then the provider was never asked
    assert_eq!(
        requests_reaching(&server).await,
        0,
        "the turn must wait on our side of the socket; a request sent here is a request queued \
         inside the provider, which is exactly the 38-minute invisible wait this replaces"
    );
}

/// The gate must open: a queue that never admits is a deadlock wearing a queue's name.
#[tokio::test]
async fn a_turn_reaches_the_model_when_the_provider_slot_is_free() {
    // Given a session on an idle provider
    let server = a_model_that_answers().await;
    let queue = ProviderQueue::new();
    let mut session = a_session_over(
        &server,
        SubagentConfig::new(a_codebase_that_answers()).with_provider_queue(queue.clone()),
    );

    // When it is prompted
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then the provider was asked
    assert_eq!(requests_reaching(&server).await, 1);
}

/// A slot held past the turn that needed it starves everyone behind it — the failure mode in
/// miniature, and the one an early return or an error path would reintroduce.
#[tokio::test]
async fn a_finished_turn_leaves_the_provider_slot_free() {
    // Given a session that has answered
    let server = a_model_that_answers().await;
    let queue = ProviderQueue::new();
    let mut session = a_session_over(
        &server,
        SubagentConfig::new(a_codebase_that_answers()).with_provider_queue(queue.clone()),
    );
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then nothing is left holding its provider
    assert_eq!(queue.in_flight(&server.uri()), 0);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// A turn queues on the endpoint it will actually call, so two agents on one local Ollama contend
/// and two on different endpoints do not.
#[tokio::test]
async fn a_turn_queues_on_the_endpoint_its_def_names() {
    // Given a session on one provider, while a different endpoint's slot is held
    let server = a_model_that_answers().await;
    let queue = ProviderQueue::new();
    let _held_elsewhere = queue
        .admit("https://api.example.invalid", SOMEONE_ELSE)
        .await
        .expect("an unrelated provider's slot is taken");
    let mut session = a_session_over(
        &server,
        SubagentConfig::new(a_codebase_that_answers()).with_provider_queue(queue.clone()),
    );

    // When it is prompted
    session
        .prompt(THE_GOAL)
        .await
        .expect("a busy unrelated endpoint must not gate this one");

    // Then it was served
    assert_eq!(requests_reaching(&server).await, 1);
}

// ─── API boundaries ──────────────────────────────────────────────────────────

/// The control, and the back-compatibility guarantee: a conversation opened without a queue
/// behaves exactly as every one did before there was one.
#[tokio::test]
async fn a_session_configured_with_no_queue_reaches_the_model_directly() {
    // Given a session with no provider queue configured
    let server = a_model_that_answers().await;
    let mut session = a_session_over(&server, SubagentConfig::new(a_codebase_that_answers()));

    // When it is prompted
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then it was served, ungated
    assert_eq!(requests_reaching(&server).await, 1);
}
