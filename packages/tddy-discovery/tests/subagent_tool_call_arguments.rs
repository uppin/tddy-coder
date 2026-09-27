//! Integration tests: a turn outcome reports what each tool call *asked for*, not only which
//! tool was called.
//!
//! `MessageDescriptor.tool_calls` is a list of tool names today. In session 01a0e200 that was
//! enough for the main agent to see that three `READ`s had failed, and not enough for it to see
//! anything else: the path those reads asked for —
//! `packages/tddy-web/src/lib/terminalGridMeasure.ts"` — appears nowhere in the outcome. The one
//! reader positioned to notice the subagent was emitting broken arguments was handed a tool name
//! and an error string, and could only conclude "wrong file, try another".
//!
//! The bound matters as much as the field. A `WRITE` carries a whole file in its arguments, and
//! an outcome enumerating several of those would cost its reader more context than the turn it
//! describes — the same reason [`MESSAGE_PREVIEW_CHARS`] bounds a message preview.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageRole, PromptOutcome, SubagentConfig, SubagentRegistry, SubagentSession,
    ToolCallDescriptor, MESSAGE_PREVIEW_CHARS,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const THE_GOAL: &str = "Find the scrollback constant";
const A_PATH: &str = "packages/tddy-web/src/lib/terminalHistoryLoader.ts";
/// Long enough that the preview bound has to cut it, and built from one repeated character so the
/// kept prefix is an exact substring.
const A_FILE_BODY_LENGTH: usize = MESSAGE_PREVIEW_CHARS * 4;

fn a_def(base_url: &str, tools: Vec<SubagentTool>) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools,
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

fn a_turn_calling(calls: Vec<(&str, serde_json::Value)>) -> serde_json::Value {
    let tool_calls: Vec<serde_json::Value> = calls
        .iter()
        .enumerate()
        .map(|(i, (name, args))| {
            serde_json::json!({
                "id": format!("call_{i}"),
                "type": "function",
                "function": { "name": name, "arguments": args.to_string() }
            })
        })
        .collect();
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": "Working.", "tool_calls": tool_calls },
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

async fn a_model_that_calls_then_answers(first_turn: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first_turn))
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

fn a_session_over(server: &MockServer, tools: Vec<SubagentTool>) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri(), tools)])
        .create("explorer", SubagentConfig::new(a_codebase_that_answers()))
        .expect("the def must resolve")
}

/// The calls the first assistant message of `outcome` reported making.
fn the_calls_reported(outcome: &PromptOutcome) -> &[ToolCallDescriptor] {
    &outcome
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Assistant && !m.tool_calls.is_empty())
        .expect("a turn that called tools appends an assistant message naming them")
        .tool_calls
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The field that was missing. A reader must be able to see the path that failed, not only that
/// a `READ` did.
#[tokio::test]
async fn an_assistant_message_reports_the_arguments_of_each_call_it_made() {
    // Given a model that reads one file
    let server = a_model_that_calls_then_answers(a_turn_calling(vec![(
        "READ",
        serde_json::json!({ "path": A_PATH }),
    )]))
    .await;
    let mut session = a_session_over(&server, vec![SubagentTool::Read]);

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then the call is reported with what it asked for
    let calls = the_calls_reported(&outcome);
    assert_eq!(calls.len(), 1, "one call was made, so one is reported");
    assert_eq!(calls[0].name, "READ");
    assert!(
        calls[0].arguments.contains(A_PATH),
        "the reader must be able to see the path the call asked for; got: {}",
        calls[0].arguments
    );
}

/// Order is information: it is how a reader sees a search narrowing, or circling.
#[tokio::test]
async fn several_calls_in_one_turn_are_reported_in_the_order_they_were_made() {
    // Given a model that globs twice and then reads
    let server = a_model_that_calls_then_answers(a_turn_calling(vec![
        ("GLOB", serde_json::json!({ "pattern": "**/*terminal*" })),
        ("GLOB", serde_json::json!({ "pattern": "**/*history*" })),
        ("READ", serde_json::json!({ "path": A_PATH })),
    ]))
    .await;
    let mut session = a_session_over(&server, vec![SubagentTool::Read, SubagentTool::Glob]);

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then all three are reported, in order
    let named: Vec<&str> = the_calls_reported(&outcome)
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(named, vec!["GLOB", "GLOB", "READ"]);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// A `WRITE`'s arguments are a whole file. Reporting them verbatim would make the outcome more
/// expensive than the work it describes.
#[tokio::test]
async fn a_calls_arguments_are_cut_to_the_preview_bound() {
    // Given a model that writes a file far longer than the preview bound
    let a_long_body = "x".repeat(A_FILE_BODY_LENGTH);
    let server = a_model_that_calls_then_answers(a_turn_calling(vec![(
        "WRITE",
        serde_json::json!({ "path": A_PATH, "contents": a_long_body }),
    )]))
    .await;
    let mut session = a_session_over(&server, vec![SubagentTool::Write]);

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then the reported arguments are bounded
    let calls = the_calls_reported(&outcome);
    assert_eq!(
        calls[0].arguments.chars().count(),
        MESSAGE_PREVIEW_CHARS,
        "arguments must be cut to the same bound a message preview is"
    );
}

/// An assistant message that only spoke reports no calls — an empty list, not an absent field, so
/// a reader can tell "said something" from "did something".
#[tokio::test]
async fn an_assistant_message_that_called_nothing_reports_an_empty_call_list() {
    // Given a model that answers without calling anything
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_final_answer("scrollback is 50000")),
        )
        .mount(&server)
        .await;
    let mut session = a_session_over(&server, vec![SubagentTool::Read]);

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then the assistant message reports no calls
    let assistant = outcome
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Assistant)
        .expect("the answer is an assistant message");
    assert_eq!(assistant.tool_calls, Vec::<ToolCallDescriptor>::new());
}
