//! Integration tests: a caller's condition on a tool call yields the subagent's turn back to
//! it, at that call.
//!
//! `maxTurns` bounds a turn's *length*; it cannot hand control back *at a point the caller
//! cares about*. A `STR_REPLACE` that matches nothing and keeps hammering the same edit burns
//! the whole budget before the caller learns anything. A yield condition names the point:
//! stop the turn at that call, keep its result in the transcript, never send it to the model,
//! and report which condition fired and which tool message the turn stopped at.
//!
//! Feature: docs/dev/1-WIP/2026-09-27-yield-conditions-prd.md (AC1–AC4)

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, PromptOutcome, StopReason, SubagentConfig, SubagentRegistry, TurnRequest, When,
    YieldCondition,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::StrReplace],
        max_turns: 2,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

/// Every tool call answers as a failed STR_REPLACE: nothing matched, nothing written.
fn a_codebase_where_nothing_matches() -> CodebaseAccess {
    let answer = serde_json::json!({
        "replaced": false,
        "matchedOccurrences": 0,
        "bytes_written": 0
    })
    .to_string();
    CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let answer = answer.clone();
            Box::pin(async move { answer })
        },
    )
}

fn a_turn_that_calls(tool: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Trying that.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": tool, "arguments": args.to_string() }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_final_answer(text: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
}

/// A model that calls STR_REPLACE once, then answers — so a turn that does NOT yield takes a
/// second model turn, and a turn that DOES yield never asks the second one.
async fn a_model_that_edits_then_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_turn_that_calls(
            "STR_REPLACE",
            serde_json::json!({
                "path": "src/lib.rs",
                "old_string": "let a = 1;",
                "new_string": "let a = 0;"
            }),
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("done")))
        .mount(&server)
        .await;
    server
}

async fn a_turn_with(request: TurnRequest) -> PromptOutcome {
    let server = a_model_that_edits_then_answers().await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create(
            "explorer",
            SubagentConfig::new(a_codebase_where_nothing_matches()),
        )
        .expect("the def must resolve");
    session.take_turn(request).await.expect("the turn runs")
}

#[tokio::test]
async fn an_outcome_condition_yields_the_turn_at_the_matching_tool_call() {
    let outcome = a_turn_with(
        TurnRequest::prompting("fix the constant").with_yield_conditions(vec![YieldCondition {
            tool: "STR_REPLACE".to_string(),
            when: When::Outcome {
                fact: tddy_discovery::subagent::OutcomeFact::MatchedLines(0),
            },
        }]),
    )
    .await;

    // The turn stopped at the call, and names which condition fired and where it stopped.
    assert_eq!(outcome.stop_reason, StopReason::YieldedToCaller);
    let fired = outcome
        .fired_condition
        .as_ref()
        .expect("the outcome names the fired condition");
    assert_eq!(fired.tool, "STR_REPLACE");
    // The tool message's id — the resume-with-replacement's anchor.
    let yielded_id = outcome
        .yielded_message_id
        .as_ref()
        .expect("the outcome names the yielded tool message");
    let tool_message = outcome
        .messages
        .iter()
        .find(|described| described.tool.is_some())
        .expect("the yielded call's result stays in the transcript");
    assert_eq!(tool_message.id, *yielded_id);
}

#[tokio::test]
async fn an_argument_condition_yields_on_a_contained_substring() {
    let outcome = a_turn_with(
        TurnRequest::prompting("fix the constant").with_yield_conditions(vec![YieldCondition {
            tool: "STR_REPLACE".to_string(),
            when: When::Argument {
                field: "old_string".to_string(),
                contains: "let a = 1;".to_string(),
            },
        }]),
    )
    .await;
    assert_eq!(
        outcome.stop_reason,
        StopReason::YieldedToCaller,
        "the call's arguments contain the needle, evaluated after the call ran"
    );
}

#[tokio::test]
async fn a_non_matching_condition_is_invisible_in_the_outcome() {
    let outcome = a_turn_with(
        TurnRequest::prompting("fix the constant").with_yield_conditions(vec![YieldCondition {
            tool: "STR_REPLACE".to_string(),
            when: When::Outcome {
                fact: tddy_discovery::subagent::OutcomeFact::MatchedLines(5),
            },
        }]),
    )
    .await;
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert!(outcome.fired_condition.is_none());
    assert!(outcome.yielded_message_id.is_none());
}

#[tokio::test]
async fn a_yielded_turn_ends_its_transcript_at_the_tool_result() {
    let server = a_model_that_edits_then_answers().await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create(
            "explorer",
            SubagentConfig::new(a_codebase_where_nothing_matches()),
        )
        .expect("the def must resolve");
    let outcome = session
        .take_turn(
            TurnRequest::prompting("fix the constant").with_yield_conditions(vec![
                YieldCondition {
                    tool: "STR_REPLACE".to_string(),
                    when: When::Outcome {
                        fact: tddy_discovery::subagent::OutcomeFact::MatchedLines(0),
                    },
                },
            ]),
        )
        .await
        .expect("the turn runs");

    assert_eq!(outcome.stop_reason, StopReason::YieldedToCaller);
    // The model is never sent the result the condition stopped at: exactly one model request
    // (the one that issued the call), never the second the mock would have answered.
    assert_eq!(
        server
            .received_requests()
            .await
            .expect("the mock provider records its requests")
            .len(),
        1,
        "a yielded turn never spends the model turn after the call"
    );
}
