//! Integration tests: every turn reports the messages it appended, so a caller can see what the
//! agent actually did — and name a point to go back to.
//!
//! The 2026-09-26 incident had two readers and neither could see anything. FastContext could not
//! tell that its 54 tool calls had all been refused, and the main agent reading its answer could
//! not tell either: a turn outcome is `{stopReason, content, usage}` and says nothing about the
//! exchanges behind it. A single `isError` on a tool result would have ended that session in one
//! turn instead of two fabricated summaries.
//!
//! Message ids are the other half. `subagent_resume` rewinds to a named message, and a caller
//! cannot name one it has never been told about.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control

use std::collections::HashSet;
use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageDescriptor, MessageRole, PromptOutcome, SubagentConfig,
    SubagentRegistry, SubagentSession, MESSAGE_PREVIEW_CHARS,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A tool error whose text carries the two characters that break naive JSON assembly. The engine
/// builds this envelope with `format!("{{\"error\": \"{e}\"}}")` today, so a message shaped like
/// this produces a tool result that is not valid JSON at all.
const AN_ERROR_WITH_A_QUOTE: &str = "READ \"src/a b.rs\": No such file\nor directory";

const A_HUGE_FILE: usize = 42_000;
const THE_GOAL: &str = "Find the scrollback constant";
const A_BUDGET: u32 = 2;

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

fn a_codebase_answering_with(answer: serde_json::Value) -> CodebaseAccess {
    CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let answer = answer.clone();
            Box::pin(async move { answer.to_string() })
        },
    )
}

fn a_turn_that_reads(path: &str) -> serde_json::Value {
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
                        "arguments": serde_json::json!({ "path": path }).to_string()
                    }
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

/// A provider that reads once and then answers.
async fn a_model_that_reads_then_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_turn_that_reads("src/terminal.tsx")),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_final_answer("scrollback is 50000")),
        )
        .mount(&server)
        .await;
    server
}

fn a_session_over(
    server: &MockServer,
    access: CodebaseAccess,
    max_turns: u32,
) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri(), max_turns)])
        .create("explorer", SubagentConfig { access })
        .expect("the def must resolve")
}

fn roles(outcome: &PromptOutcome) -> Vec<MessageRole> {
    outcome.messages.iter().map(|m| m.role).collect()
}

fn the_tool_result(outcome: &PromptOutcome) -> &MessageDescriptor {
    outcome
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("a turn that called a tool appends its result")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// A turn accounts for itself: the prompt, the assistant's tool call, the tool's answer, and the
/// assistant's conclusion, in the order they happened.
#[tokio::test]
async fn a_turn_outcome_lists_the_messages_it_appended() {
    // Given an agent whose tool answers
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(
        &server,
        a_codebase_answering_with(serde_json::json!({
            "content": "const PAGE_SCROLLBACK = 50000;",
            "truncated": false,
            "total_lines": 1,
        })),
        A_BUDGET,
    );

    // When it reads a file and answers
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then the exchange is reported in order
    assert_eq!(
        roles(&outcome),
        vec![
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Tool,
            MessageRole::Assistant,
        ],
        "the caller must be able to see the prompt, the call, the result and the conclusion"
    );
    assert_eq!(
        outcome.messages[1].tool_calls,
        vec!["READ".to_string()],
        "an assistant message names the tools it called"
    );
    assert_eq!(the_tool_result(&outcome).tool.as_deref(), Some("READ"));
}

/// The field whose absence let the incident run for 54 calls. A refused tool call is marked as
/// one, and its text survives characters that break naive JSON assembly.
#[tokio::test]
async fn a_failed_tool_result_is_marked_and_its_error_text_survives_quoting() {
    // Given a tool whose error message contains a quote and a newline
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(
        &server,
        a_codebase_answering_with(
            serde_json::json!({ "is_error": true, "error": AN_ERROR_WITH_A_QUOTE }),
        ),
        A_BUDGET,
    );

    // When the agent calls it
    let outcome = session
        .prompt(THE_GOAL)
        .await
        .expect("one failed read is not an outage");

    // Then the result is flagged, and the error reached the caller intact
    let result = the_tool_result(&outcome);
    assert!(
        result.is_error,
        "a refused tool call must be visibly refused; this is what a main agent reads to see \
         that a subagent is failing rather than progressing"
    );
    assert!(
        result.preview.contains("No such file"),
        "the tool's own words must survive the envelope, got: {}",
        result.preview
    );
}

/// A preview is a handle for choosing a rewind point, not a copy of the payload. One `Read` result
/// in the incident was 42 KB, and two of them crossed the wire for the same file.
#[tokio::test]
async fn a_large_tool_result_is_previewed_rather_than_carried_whole() {
    // Given a tool returning a file far larger than a preview
    let server = a_model_that_reads_then_answers().await;
    let huge = "x".repeat(A_HUGE_FILE);
    let mut session = a_session_over(
        &server,
        a_codebase_answering_with(serde_json::json!({
            "content": huge,
            "truncated": false,
            "total_lines": 1,
        })),
        A_BUDGET,
    );

    // When the agent reads it
    let outcome = session.prompt(THE_GOAL).await.expect("an answer");

    // Then the descriptor carries a handle, not the payload
    let preview = &the_tool_result(&outcome).preview;
    assert!(
        preview.chars().count() <= MESSAGE_PREVIEW_CHARS,
        "a preview of {} characters is a copy, not a preview",
        preview.chars().count()
    );
    assert!(
        !preview.is_empty(),
        "a preview must still say something about the message it stands for"
    );
}

/// Ids identify a message for the life of the conversation. Two turns must not both mint `m1`,
/// or a rewind addresses whichever the implementation happens to find first.
#[tokio::test]
async fn message_ids_are_unique_across_the_whole_conversation() {
    // Given an agent that takes two turns
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("first")))
        .mount(&server)
        .await;
    let mut session = a_session_over(
        &server,
        a_codebase_answering_with(
            serde_json::json!({ "content": "", "truncated": false, "total_lines": 0 }),
        ),
        A_BUDGET,
    );

    // When it is prompted twice
    let first = session.prompt("first question").await.expect("an answer");
    let second = session.prompt("second question").await.expect("an answer");

    // Then no id is handed out twice
    let mut seen: HashSet<String> = HashSet::new();
    for descriptor in first.messages.iter().chain(second.messages.iter()) {
        assert!(
            seen.insert(descriptor.id.as_str().to_string()),
            "id {} was minted twice; a rewind to it would be ambiguous",
            descriptor.id.as_str()
        );
    }
    assert!(
        !seen.is_empty(),
        "a turn that appended messages must report them"
    );
}
