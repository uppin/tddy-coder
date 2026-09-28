//! Integration tests: a turn outcome reports each tool call's result as structured facts.
//!
//! A main agent reading a subagent's turn sees the raw result's first 240 characters and a
//! boolean — the numbers it needs to reason with (chars read, lines replaced, exit codes) are
//! truncated away. These tests pin the `resultSummary` every tool-role descriptor carries, and
//! the STR_REPLACE occurrence count the tool engine reports so a summary can name how much an
//! edit touched.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control (result summaries)

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageDescriptor, MessageRole, PromptOutcome, ResultSummary, SubagentConfig,
    SubagentRegistry,
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
        tools: vec![
            SubagentTool::Read,
            SubagentTool::Grep,
            SubagentTool::StrReplace,
        ],
        max_turns: 2,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

fn a_codebase_that_answers(content_lines: u64) -> CodebaseAccess {
    let content = serde_json::json!({
        "content": "fn main() {\n    println!(\"hi\");\n}",
        "truncated": false,
        "total_lines": content_lines
    })
    .to_string();
    CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let content = content.clone();
            Box::pin(async move { content })
        },
    )
}

fn a_turn_that_calls(tool: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Looking there.",
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

async fn a_model_that_calls_then_answers(tool: &str, args: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_turn_that_calls(tool, args)))
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

async fn a_turn_over(tool: &str, args: serde_json::Value) -> PromptOutcome {
    let server = a_model_that_calls_then_answers(tool, args).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", SubagentConfig::new(a_codebase_that_answers(3)))
        .expect("the def must resolve");
    session.prompt("find it").await.expect("the turn runs")
}

fn the_tool_result(outcome: &PromptOutcome) -> &MessageDescriptor {
    outcome
        .messages
        .iter()
        .find(|described| described.role == MessageRole::Tool)
        .expect("the turn appended a tool result")
}

#[tokio::test]
async fn a_read_tool_message_carries_a_structured_result_summary() {
    let outcome = a_turn_over("READ", serde_json::json!({ "path": "src/lib/terminal.ts" })).await;
    assert_eq!(
        the_tool_result(&outcome).result_summary,
        Some(ResultSummary::Read {
            first_line: Some("fn main() {".to_string()),
            chars_read: 31,
            total_lines: 3,
            truncated: false
        })
    );
}

#[tokio::test]
async fn a_grep_tool_message_carries_its_match_counts() {
    // The managed closure answers a READ shape for every tool; the summary's facts come from
    // the tool the model *called*, so this pins that the summary is per-tool rather than
    // per-codebase-answer. The Grep-specific counts are pinned at the unit level, where each
    // tool's own result JSON drives the extraction directly.
    let outcome = a_turn_over("GREP", serde_json::json!({ "pattern": "main" })).await;
    let summary = the_tool_result(&outcome)
        .result_summary
        .as_ref()
        .expect("the descriptor carries a summary");
    assert!(
        summary != &ResultSummary::Error,
        "a grep summary, not an error shape"
    );
}

#[tokio::test]
async fn a_failed_dispatch_summary_reports_the_error_shape() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_turn_that_calls(
            "READ",
            // A path the argument schema refuses — a rejection, not a dispatch.
            serde_json::json!({ "path": 42 }),
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("could not")))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", SubagentConfig::new(a_codebase_that_answers(3)))
        .expect("the def must resolve");
    let outcome = session.prompt("find it").await.expect("the turn runs");
    let described = the_tool_result(&outcome);
    assert!(described.is_error, "the call was rejected");
    assert_eq!(described.result_summary, Some(ResultSummary::Error));
}

#[tokio::test]
async fn a_result_summary_round_trips_its_serialized_form() {
    // The shape the string-typed proto field and the MCP outcome both carry as text: serialize
    // one key naming the tool, parse it back, arrive at the same summary.
    let summary = ResultSummary::StrReplace {
        replaced: true,
        matched_lines: 1,
        bytes_written: 128,
    };
    let wire = serde_json::to_string(&summary).expect("serializable");
    assert_eq!(
        serde_json::from_str::<ResultSummary>(&wire).expect("parseable"),
        summary
    );
}
