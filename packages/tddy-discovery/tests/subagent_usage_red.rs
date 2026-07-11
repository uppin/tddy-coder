//! Unit/integration tests: token-usage capture in the OpenAI-compatible client and cumulative
//! usage accounting across a stateful subagent conversation.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (requirements 1-2)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! Both FastContext-style endpoints and local models served by Ollama report a `usage` object on
//! `/v1/chat/completions` (`prompt_tokens` / `completion_tokens`). These tests pin that the client
//! surfaces it, that an omitted `usage` is treated as zero rather than an error, and that a
//! subagent session sums each turn's usage into a per-conversation running total.

use tddy_discovery::openai::{ChatCompletionRequest, ChatMessage, OpenAiClient, TokenUsage};
use tddy_discovery::subagent::{CodebaseAccess, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn final_answer_with_usage(
    answer: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
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

async fn mount_always(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

// ─── OpenAiClient usage parsing ────────────────────────────────────────────────

/// The client maps the response `usage {prompt_tokens, completion_tokens}` onto `TokenUsage`
/// (`prompt_tokens` → input, `completion_tokens` → output).
#[tokio::test]
async fn openai_client_parses_prompt_and_completion_token_usage_from_a_response() {
    // Given
    let server = MockServer::start().await;
    mount_always(&server, final_answer_with_usage("src/a.rs:1-1", 120, 45)).await;
    let client = OpenAiClient::new(server.uri());
    let request = ChatCompletionRequest {
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        messages: vec![ChatMessage::user("Find the entry point")],
        tools: vec![],
        tool_choice: serde_json::json!("auto"),
        temperature: 0.0,
    };

    // When
    let response = client
        .complete(request)
        .await
        .expect("complete must succeed");

    // Then
    assert_eq!(
        response.usage,
        Some(TokenUsage {
            input_tokens: 120,
            output_tokens: 45,
        })
    );
}

/// A response with no `usage` object is not an error — `usage` is simply absent, and callers treat
/// that as zero tokens.
#[tokio::test]
async fn openai_client_reports_no_usage_when_the_response_omits_it() {
    // Given
    let server = MockServer::start().await;
    mount_always(
        &server,
        serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        }),
    )
    .await;
    let client = OpenAiClient::new(server.uri());
    let request = ChatCompletionRequest {
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        messages: vec![ChatMessage::user("hello")],
        tools: vec![],
        tool_choice: serde_json::json!("auto"),
        temperature: 0.0,
    };

    // When
    let response = client
        .complete(request)
        .await
        .expect("complete must succeed");

    // Then
    assert_eq!(response.usage, None);
}

// ─── Per-conversation accumulation ─────────────────────────────────────────────

/// Each `prompt()` call folds its turn's usage into a running per-conversation total, so a session
/// prompted twice reports the field-wise sum of both turns.
#[tokio::test]
async fn subagent_session_accumulates_token_usage_across_two_prompts() {
    // Given — every turn returns a final answer reporting 100 prompt + 40 completion tokens.
    let server = MockServer::start().await;
    mount_always(&server, final_answer_with_usage("src/a.rs:1-1", 100, 40)).await;
    let mut session = SubagentRegistry::new()
        .create("fastcontext", a_local_config(&server.uri()))
        .expect("fastcontext must be registered");

    // When
    session
        .prompt("Where is the entry point?")
        .await
        .expect("first prompt");
    session
        .prompt("And the shutdown path?")
        .await
        .expect("second prompt");

    // Then
    assert_eq!(
        session.cumulative_usage(),
        TokenUsage {
            input_tokens: 200,
            output_tokens: 80,
        }
    );
}

/// A single `prompt()` reports that turn's own token usage on its outcome, so the caller can show
/// per-turn cost inline.
#[tokio::test]
async fn subagent_prompt_reports_the_turns_token_usage_on_the_outcome() {
    // Given
    let server = MockServer::start().await;
    mount_always(&server, final_answer_with_usage("src/a.rs:1-1", 100, 40)).await;
    let mut session = SubagentRegistry::new()
        .create("fastcontext", a_local_config(&server.uri()))
        .expect("fastcontext must be registered");

    // When
    let outcome = session
        .prompt("Where is the entry point?")
        .await
        .expect("prompt");

    // Then
    assert_eq!(
        outcome.usage,
        TokenUsage {
            input_tokens: 100,
            output_tokens: 40,
        }
    );
}
