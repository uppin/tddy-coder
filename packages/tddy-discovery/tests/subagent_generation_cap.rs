//! Integration tests: a subagent turn bounds how much the model may generate, and a turn the
//! provider cut at that bound says so instead of passing a half-written answer off as finished.
//!
//! Session 01a0e200 did not hang on a tool call. Its model entered a degenerate generation and
//! never emitted a stop token: `llama-server` reported `n_gen = 19790` and climbing at 24 t/s,
//! thirteen minutes in, having already context-shifted once (`n_discard = 16381`, `n_keep = 4`)
//! — which discarded the whole prompt and guaranteed it would never recover. Nothing upstream
//! could end it. `ChatCompletionRequest` carries no `max_tokens`; the turn budget counts *turns*,
//! so it cannot fire inside one; and `IN_JAIL_TOOL_TIMEOUT` bounds a tool call, not inference.
//! The main agent sat in `subagent_await` while the session burned 128% CPU and 10 GB.
//!
//! The second half matters as much as the first. `finish_reason` is parsed today and only logged
//! (`subagent.rs`), so a provider that stops at a token limit produces an outcome shaped exactly
//! like a completed one — a truncated answer a caller has no way to tell from a whole one.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, StopReason, SubagentConfig, SubagentRegistry, SubagentSession,
    SUBAGENT_MAX_OUTPUT_TOKENS,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const THE_GOAL: &str = "Find the scrollback constant";
/// One turn of tool calls, so the budget is spent and the synthesis turn has to run.
const A_ONE_TURN_BUDGET: u32 = 1;

fn a_def(base_url: &str, max_turns: u32) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns,
        replaces: Vec::new(),
        usage_notes: None,
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

fn a_turn_that_reads() -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Reading.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "READ",
                        "arguments": "{\"path\": \"src/terminal.tsx\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn an_answer_with_finish_reason(text: &str, finish_reason: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": text },
            "finish_reason": finish_reason
        }]
    })
}

async fn a_model_responding_with(body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn a_session_over(server: &MockServer, max_turns: u32) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri(), max_turns)])
        .create("explorer", SubagentConfig::new(a_codebase_that_answers()))
        .expect("the def must resolve")
}

/// The `max_tokens` every request the provider received carried, in order — `None` for a request
/// that carried none, which is what makes an unbounded turn visible rather than inferred.
async fn output_caps_sent_to(server: &MockServer) -> Vec<Option<u64>> {
    server
        .received_requests()
        .await
        .expect("the mock provider records its requests")
        .iter()
        .map(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("the request body is JSON");
            body.get("max_tokens").and_then(|v| v.as_u64())
        })
        .collect()
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The bound that was missing. Without it a model that never emits a stop token generates until
/// someone kills the process.
#[tokio::test]
async fn a_turn_tells_the_provider_how_many_tokens_it_may_generate() {
    // Given a model that answers immediately
    let server =
        a_model_responding_with(an_answer_with_finish_reason("scrollback is 50000", "stop")).await;
    let mut session = a_session_over(&server, 2);

    // When it runs a turn
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then the request carried the cap
    assert_eq!(
        output_caps_sent_to(&server).await,
        vec![Some(u64::from(SUBAGENT_MAX_OUTPUT_TOKENS))]
    );
}

/// The synthesis turn is a separate request and runs after the budget is spent — exactly the
/// position session 01a0e200's runaway occupied, so an unbounded one leaves the hole open.
#[tokio::test]
async fn the_synthesis_turn_is_bounded_by_the_same_cap() {
    // Given a model that only ever calls tools, and a one-turn budget
    let server = a_model_responding_with(a_turn_that_reads()).await;
    let mut session = a_session_over(&server, A_ONE_TURN_BUDGET);

    // When the budget is spent and the agent is asked to summarise
    session
        .prompt(THE_GOAL)
        .await
        .expect("a spent budget still yields an outcome");

    // Then both the tool turn and the synthesis turn carried the cap
    let cap = Some(u64::from(SUBAGENT_MAX_OUTPUT_TOKENS));
    assert_eq!(output_caps_sent_to(&server).await, vec![cap, cap]);
}

// ─── Error scenarios ─────────────────────────────────────────────────────────

/// `finish_reason` is parsed and only logged today, so a model cut off mid-sentence produces an
/// outcome indistinguishable from one that finished.
#[tokio::test]
async fn a_turn_the_provider_cut_at_the_token_limit_is_not_reported_as_a_finished_answer() {
    // Given a model whose answer is cut off at the cap
    let server = a_model_responding_with(an_answer_with_finish_reason(
        "the constant is defined in",
        "length",
    ))
    .await;
    let mut session = a_session_over(&server, 2);

    // When it runs a turn
    let outcome = session.prompt(THE_GOAL).await.expect("an outcome");

    // Then the caller is told the answer was cut, not that the agent finished
    assert_eq!(outcome.stop_reason, StopReason::MaxTokens);
}

/// A cut answer is still work. Discarding it would make the caller re-run the whole turn to
/// recover what the model already said.
#[tokio::test]
async fn a_turn_cut_at_the_token_limit_still_returns_what_the_model_produced() {
    // Given a model whose answer is cut off at the cap
    let server = a_model_responding_with(an_answer_with_finish_reason(
        "the constant is defined in",
        "length",
    ))
    .await;
    let mut session = a_session_over(&server, 2);

    // When it runs a turn
    let outcome = session.prompt(THE_GOAL).await.expect("an outcome");

    // Then the partial content survives
    assert_eq!(
        outcome
            .content
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>(),
        vec!["the constant is defined in"]
    );
}
