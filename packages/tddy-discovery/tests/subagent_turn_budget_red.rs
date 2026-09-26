//! Integration tests: the caller may choose a subagent's turn budget for one call, within a
//! ceiling.
//!
//! Today the budget is the definition's alone. `SubagentConfig`'s own doc says why —
//! *"The endpoint, model, credential and turn budget are not here — they are the def's, and a
//! caller able to override them could run a session against a model the operator never
//! configured"* — and `subagent_new_session`'s handler repeats it for the roster entry. Both are
//! about the *operator's* configuration not being editable by a running session, and a per-call
//! budget does not touch that: it chooses how long this search may take, not what it runs against.
//!
//! What it does risk is unbounded spend, so it is clamped. A caller that asks for more than the
//! ceiling gets the ceiling **and is told**, because a budget silently smaller than the one
//! requested is how a caller concludes the agent gave up early.
//!
//! In the 2026-09-26 incident the budget was the registry default of 10 — hardwired in
//! `assistant_def.rs`, with no column in `models.db` to change it — and ten turns was both too
//! many (all of them failed) and, for a real search, too few.
//!
//! Changeset: docs/dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure.md

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, SubagentConfig, SubagentRegistry, SubagentSession, TurnRequest,
    SUBAGENT_MAX_TURNS_CEILING,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The definition's budget in these tests — deliberately small, so a caller's override is
/// unmistakable in a request count.
const THE_DEFINITIONS_BUDGET: u32 = 2;

/// What a caller asks for when it wants the chain finished.
const A_LONGER_BUDGET: u32 = 7;

const THE_GOAL: &str = "Trace every caller of spawn_session";

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
    }
}

fn a_codebase_that_answers() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({ "content": "fn spawn_session() {}", "truncated": false, "total_lines": 1 })
                    .to_string()
            })
        },
    )
}

/// A model that never stops reaching for tools, so the only thing that ends a prompt is its
/// budget — which makes the request count a direct reading of the budget that applied.
async fn a_model_that_never_finishes() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Still looking.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "GREP",
                            "arguments": serde_json::json!({ "pattern": "spawn_session" }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })))
        .mount(&server)
        .await;
    server
}

async fn turns_the_model_was_asked_for(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock provider records its requests")
        .len()
}

fn a_session_over(server: &MockServer, max_turns: u32) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri(), max_turns)])
        .create(
            "explorer",
            SubagentConfig {
                access: a_codebase_that_answers(),
            },
        )
        .expect("the def must resolve")
}

/// A turn budget spent entirely on tool calls costs the budget plus the one synthesis turn.
fn turns_for_a_budget_of(budget: u32) -> usize {
    budget as usize + 1
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The want: a main agent that can see the chain was cut short asks for it to be finished.
#[tokio::test]
async fn a_caller_supplied_budget_replaces_the_definitions_for_that_call() {
    // Given an agent whose definition allows two turns
    let server = a_model_that_never_finishes().await;
    let mut session = a_session_over(&server, THE_DEFINITIONS_BUDGET);

    // When the caller asks for seven
    session
        .take_turn(TurnRequest::prompting(THE_GOAL).within_turns(A_LONGER_BUDGET))
        .await
        .expect("a longer search still answers");

    // Then seven is what it got
    assert_eq!(
        turns_the_model_was_asked_for(&server).await,
        turns_for_a_budget_of(A_LONGER_BUDGET)
    );
}

/// "For that call" is the whole of the contract with the definition: the override is not sticky,
/// so a caller cannot quietly re-configure an agent by prompting it once.
#[tokio::test]
async fn a_caller_supplied_budget_does_not_outlive_the_call_that_set_it() {
    // Given an agent whose definition allows two turns, prompted once with a larger budget
    let server = a_model_that_never_finishes().await;
    let mut session = a_session_over(&server, THE_DEFINITIONS_BUDGET);
    session
        .take_turn(TurnRequest::prompting(THE_GOAL).within_turns(A_LONGER_BUDGET))
        .await
        .expect("a longer search still answers");
    let after_the_long_one = turns_the_model_was_asked_for(&server).await;

    // When it is prompted again with no budget of its own
    session
        .prompt("and now the callers of stop_session")
        .await
        .expect("an answer");

    // Then the definition's budget applies again
    assert_eq!(
        turns_the_model_was_asked_for(&server).await - after_the_long_one,
        turns_for_a_budget_of(THE_DEFINITIONS_BUDGET),
        "the override must not have stuck to the conversation"
    );
}

/// A ceiling, because an unbounded caller-set budget is unbounded local-model time. Clamping
/// rather than refusing keeps an over-eager caller working; reporting the clamp keeps it honest.
#[tokio::test]
async fn a_budget_above_the_ceiling_is_clamped_and_the_outcome_says_so() {
    // Given a caller asking for ten times the ceiling
    let server = a_model_that_never_finishes().await;
    let mut session = a_session_over(&server, THE_DEFINITIONS_BUDGET);
    let asked_for = SUBAGENT_MAX_TURNS_CEILING * 10;

    // When the turn runs
    let outcome = session
        .take_turn(TurnRequest::prompting(THE_GOAL).within_turns(asked_for))
        .await
        .expect("an over-large budget is clamped, not refused");

    // Then it ran to the ceiling and said that is what happened
    assert_eq!(
        turns_the_model_was_asked_for(&server).await,
        turns_for_a_budget_of(SUBAGENT_MAX_TURNS_CEILING)
    );
    assert_eq!(
        outcome.clamped_max_turns,
        Some(SUBAGENT_MAX_TURNS_CEILING),
        "a caller given less than it asked for must be told, or it reads an early stop as a \
         finished search"
    );
}

/// The symmetric case: a budget within the ceiling is not reported as clamped, so the field means
/// something when it is set.
#[tokio::test]
async fn a_budget_within_the_ceiling_is_not_reported_as_clamped() {
    // Given a caller asking for a modest budget
    let server = a_model_that_never_finishes().await;
    let mut session = a_session_over(&server, THE_DEFINITIONS_BUDGET);

    // When the turn runs
    let outcome = session
        .take_turn(TurnRequest::prompting(THE_GOAL).within_turns(A_LONGER_BUDGET))
        .await
        .expect("an answer");

    // Then nothing was clamped
    assert_eq!(outcome.clamped_max_turns, None);
}
