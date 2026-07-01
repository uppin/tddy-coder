//! Unit/integration tests: stateful subagent sessions + the pluggable subagent registry.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 1-3)
//! Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md
//!
//! A `SubagentSession` is a *stateful* conversation: unlike `FastContextBackend::invoke` (one-shot
//! per `InvokeRequest`), `prompt()` can be called repeatedly against the same session and must see
//! prior turns — this is what lets the main agent ping-pong codebase questions against one
//! conversation id instead of re-explaining context on every call.

use tddy_discovery::subagent::{CodebaseAccess, StopReason, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn final_answer_response(answer: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

fn tool_call_response(tool_name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": args.to_string()
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_local_config(base_url: &str) -> SubagentConfig {
    SubagentConfig {
        base_url: base_url.to_string(),
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        max_turns: 6,
        access: CodebaseAccess::Local,
    }
}

// ─── SubagentRegistry ──────────────────────────────────────────────────────────

/// `SubagentRegistry` resolves the built-in `"fastcontext"` name to a working session.
#[tokio::test]
async fn subagent_registry_creates_a_fastcontext_session_for_the_registered_name() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/lib.rs:1-1")),
        )
        .mount(&server)
        .await;
    let registry = SubagentRegistry::new();

    // When
    let session = registry.create("fastcontext", a_local_config(&server.uri()));

    // Then
    assert!(
        session.is_ok(),
        "registry must resolve the built-in 'fastcontext' name; got: {:?}",
        session.err()
    );
}

/// An unregistered subagent name is a typed error, not a panic or a silent default backend.
#[tokio::test]
async fn subagent_registry_returns_an_error_for_an_unknown_subagent_name() {
    // Given
    let registry = SubagentRegistry::new();
    let config = a_local_config("http://127.0.0.1:1");

    // When
    let result = registry.create("not-a-real-subagent", config);

    // Then
    assert!(
        result.is_err(),
        "registry must error on an unknown subagent name, not silently substitute one"
    );
    let message = result.err().unwrap().to_string();
    assert!(
        message.contains("not-a-real-subagent"),
        "error message must name the unknown subagent; got: {message:?}"
    );
}

// ─── FastContextSession: statefulness + stop reasons ──────────────────────────

/// A second `prompt()` call on the same session must include the first call's user message and
/// the model's first answer in the request sent to the model — proving the conversation is
/// retained across calls rather than reset each time.
#[tokio::test]
async fn fast_context_session_retains_history_across_two_prompts() {
    // Given — every request gets an immediate final answer
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/a.rs:1-1")),
        )
        .mount(&server)
        .await;
    let registry = SubagentRegistry::new();
    let mut session = registry
        .create("fastcontext", a_local_config(&server.uri()))
        .expect("fastcontext must be registered");

    // When
    session
        .prompt("Where is the entry point?")
        .await
        .expect("first prompt must succeed");
    session
        .prompt("And where is the shutdown path?")
        .await
        .expect("second prompt must succeed");

    // Then — the second request's message list contains both prior turns plus the new prompt
    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 2, "exactly two model calls must be made");
    let second_body: serde_json::Value =
        serde_json::from_slice(&calls[1].body).expect("second request body must be valid JSON");
    let messages = second_body["messages"]
        .as_array()
        .expect("request body must carry a messages array");
    let user_texts: Vec<&str> = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .collect();
    assert_eq!(
        user_texts,
        vec![
            "Where is the entry point?",
            "And where is the shutdown path?"
        ],
        "second call's message history must include both user prompts, in order; got: {user_texts:?}"
    );
}

/// When the model produces a `<final_answer>`, `prompt()` yields with `StopReason::EndTurn` and
/// the citations as the response content.
#[tokio::test]
async fn fast_context_session_prompt_returns_end_turn_when_final_answer_is_produced() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/auth.rs:1-50")),
        )
        .mount(&server)
        .await;
    let registry = SubagentRegistry::new();
    let mut session = registry
        .create("fastcontext", a_local_config(&server.uri()))
        .expect("fastcontext must be registered");

    // When
    let outcome = session
        .prompt("Where is the authentication logic?")
        .await
        .expect("prompt must succeed when the model produces a final answer");

    // Then
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert_eq!(
        outcome.content.len(),
        1,
        "exactly one content block expected"
    );
    assert_eq!(outcome.content[0].text, "src/auth.rs:1-50");
}

/// When the per-prompt turn budget is exhausted with no `<final_answer>`, `prompt()` yields with
/// `StopReason::MaxTurnRequests` instead of looping forever or panicking.
#[tokio::test]
async fn fast_context_session_prompt_returns_max_turn_requests_when_turn_budget_is_exhausted() {
    // Given — the model always returns a tool call, never a final answer
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
            "GLOB",
            serde_json::json!({"pattern": "**/*.rs"}),
        )))
        .mount(&server)
        .await;
    let registry = SubagentRegistry::new();
    let mut config = a_local_config(&server.uri());
    config.max_turns = 3;
    let mut session = registry
        .create("fastcontext", config)
        .expect("fastcontext must be registered");

    // When
    let outcome = session
        .prompt("Find all Rust files")
        .await
        .expect("prompt must return Ok even when the turn budget is exhausted");

    // Then
    assert_eq!(outcome.stop_reason, StopReason::MaxTurnRequests);
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        calls.len(),
        3,
        "exactly max_turns model calls must be made for this single prompt() call"
    );
}
