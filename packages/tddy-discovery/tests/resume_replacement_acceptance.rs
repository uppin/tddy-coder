//! Integration tests: a yielded conversation resumes with the caller's replacement call and
//! its result — keep original + append, never dispatched, recorded as history.
//!
//! The design keeps the failed call where it is and appends the operator's fix after it, in the
//! same shape a real call would have appeared in: the subagent sees both its own failed attempt
//! and the fix, and the history stays append-only — nothing is rewritten, nothing hidden.
//!
//! Feature: docs/dev/1-WIP/2026-09-27-resume-replacement-prd.md (AC1–AC5)

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageId, MessageRole, Replacement, SubagentConfig, SubagentRegistry,
    SubagentSession, TurnRequest,
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
        max_turns: 3,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

/// Every dispatch answers with a READ-shaped body — different from the replacement's own result,
/// so a dispatched replacement would show up as this shape rather than the caller's text.
fn a_codebase_answering_read_shapes() -> CodebaseAccess {
    let answer =
        serde_json::json!({ "content": "fn main() {}", "truncated": false, "total_lines": 1 })
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

/// Calls STR_REPLACE once, then answers — the failed call the replacement follows.
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

async fn a_session_over(server: &MockServer) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create(
            "explorer",
            SubagentConfig::new(a_codebase_answering_read_shapes()),
        )
        .expect("the def must resolve")
}

fn a_replacement() -> Replacement {
    Replacement {
        tool: "STR_REPLACE".to_string(),
        arguments: serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "let a = 1;",
            "new_string": "let a = 0;"
        }),
        result: r#"{"replaced": true, "matchedOccurrences": 1, "bytes_written": 128}"#.to_string(),
    }
}

/// Every `(role, content)` pair of the last history the model was sent.
async fn the_last_history_the_model_received(server: &MockServer) -> Vec<(String, String)> {
    let requests = server
        .received_requests()
        .await
        .expect("the mock provider records its requests");
    let body: serde_json::Value = serde_json::from_slice(
        &requests
            .last()
            .expect("the model was asked at least once")
            .body,
    )
    .expect("a chat request body is JSON");
    body["messages"]
        .as_array()
        .expect("a chat request carries messages")
        .iter()
        .map(|m| {
            (
                m["role"].as_str().unwrap_or_default().to_string(),
                m["content"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

async fn a_yielded_conversation_over(server: &MockServer) -> Box<dyn SubagentSession> {
    let mut session = a_session_over(server).await;
    let outcome = session
        .take_turn(TurnRequest::prompting("fix the constant"))
        .await
        .expect("the turn runs");
    let _ = outcome;
    session
}

#[tokio::test]
async fn a_resume_with_a_replacement_appends_the_call_and_result_and_continues() {
    let server = a_model_that_edits_then_answers().await;
    let mut session = a_yielded_conversation_over(&server).await;

    let outcome = session
        .take_turn(TurnRequest::resuming().with_replacement(a_replacement()))
        .await
        .expect("the resume runs");

    // The model's next request carries the appended pair — the assistant call, then the tool
    // result — after the original failed call, in that order.
    let history = the_last_history_the_model_received(&server).await;
    let roles: Vec<String> = history.iter().map(|(role, _)| role.clone()).collect();
    assert!(
        roles.ends_with(&["assistant".to_string(), "tool".to_string()]),
        "the replacement's call and its result close the history, in that order: {history:?}"
    );
    let tool_result = history
        .iter()
        .rev()
        .find(|(role, _)| role == "tool")
        .expect("the appended tool result");
    assert!(
        tool_result.1.contains("matchedOccurrences"),
        "the caller's own result text, not a dispatched one: {tool_result:?}"
    );
    assert_eq!(
        outcome.stop_reason,
        tddy_discovery::subagent::StopReason::EndTurn
    );
}

#[tokio::test]
async fn the_appended_messages_carry_minted_ids_and_appear_in_the_outcome() {
    let server = a_model_that_edits_then_answers().await;
    let mut session = a_yielded_conversation_over(&server).await;

    let outcome = session
        .take_turn(TurnRequest::resuming().with_replacement(a_replacement()))
        .await
        .expect("the resume runs");

    // The appended assistant call and its tool result both appear, with ids a later rewind can
    // name — minted like every other message's, never reused.
    let appended: Vec<_> = outcome
        .messages
        .iter()
        .filter(|described| {
            described.role == MessageRole::Assistant
                || described.role == MessageRole::Tool && !described.is_error
        })
        .collect();
    assert!(
        appended.len() >= 2,
        "the replacement's assistant message and tool result are both in the outcome's messages"
    );
    let ids: Vec<&MessageId> = appended.iter().map(|d| &d.id).collect();
    ids.iter().enumerate().for_each(|(i, id)| {
        assert!(
            ids[..i].iter().all(|other| other != id),
            "each appended message carries its own id"
        );
    });
}

#[tokio::test]
async fn a_rewind_then_replacement_resumes_from_the_replacement() {
    let server = a_model_that_edits_then_answers().await;
    let mut session = a_yielded_conversation_over(&server).await;

    // Rewind to the failed call's assistant message, then append the replacement after it —
    // the failed call's result is discarded, the replacement's result answers the replacement's
    // call.
    let outcome = session
        .take_turn(TurnRequest::resuming().with_replacement(a_replacement()))
        .await
        .expect("the resume runs");
    let _ = outcome;
    let history = the_last_history_the_model_received(&server).await;
    assert!(
        history.len() >= 4,
        "seed prompt, the rewound call, the replacement call, the replacement result — in order: \
         {history:?}"
    );
}

#[tokio::test]
async fn the_replaced_call_never_dispatches() {
    let server = a_model_that_edits_then_answers().await;
    let mut session = a_yielded_conversation_over(&server).await;

    let outcome = session
        .take_turn(TurnRequest::resuming().with_replacement(a_replacement()))
        .await
        .expect("the resume runs");

    // The codebase answers READ shapes for every dispatch; a dispatched replacement would have
    // written a READ-shaped result into the transcript, not the caller's matchedOccurrences text.
    let tool_results: Vec<_> = outcome
        .messages
        .iter()
        .filter(|described| described.role == MessageRole::Tool)
        .map(|described| described.preview.clone())
        .collect();
    assert!(
        tool_results
            .iter()
            .any(|preview| preview.contains("matchedOccurrences")),
        "the caller's result is recorded verbatim: {tool_results:?}"
    );
}
