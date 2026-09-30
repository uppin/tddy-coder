//! A mutating tool call's turn outcome says what it did to the conversation's own worktree.
//!
//! The daemon runs a conversation's call in that conversation's worktree and answers with the
//! tool's result plus `worktreeChange`. These tests pin that the change reaches the caller — on the
//! tool-role descriptor of the turn outcome — and that a read reports none.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md

use std::pin::Pin;

use serde_json::json;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, FileCounts, LineCounts, MessageDescriptor, MessageRole, PromptOutcome,
    SubagentConfig, SubagentRegistry, WorktreeChange,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "coder".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Write, SubagentTool::Shell],
        max_turns: 2,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

/// A codebase whose every call answers `result` — standing in for the daemon, which answers a
/// conversation's mutating call with the tool's own fields plus `worktreeChange`.
fn a_codebase_answering(result: serde_json::Value) -> CodebaseAccess {
    let result = result.to_string();
    CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let result = result.clone();
            Box::pin(async move { result })
        },
    )
}

fn a_turn_that_calls(tool: &str, args: serde_json::Value) -> serde_json::Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Doing it.",
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

fn a_final_answer() -> serde_json::Value {
    json!({
        "choices": [{
            "message": { "role": "assistant", "content": "done" },
            "finish_reason": "stop"
        }]
    })
}

async fn a_turn_where_the_model_calls(
    tool: &str,
    args: serde_json::Value,
    answered_with: serde_json::Value,
) -> PromptOutcome {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_turn_that_calls(tool, args)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer()))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create(
            "coder",
            SubagentConfig::new(a_codebase_answering(answered_with)),
        )
        .expect("the def must resolve");
    session
        .prompt("make the change")
        .await
        .expect("the turn runs")
}

fn the_tool_result(outcome: &PromptOutcome) -> &MessageDescriptor {
    outcome
        .messages
        .iter()
        .find(|described| described.role == MessageRole::Tool)
        .expect("the turn appended a tool result")
}

#[tokio::test]
async fn a_mutating_call_reports_its_worktree_change_in_the_turn_outcome() {
    // Given / When
    let outcome = a_turn_where_the_model_calls(
        "WRITE",
        json!({ "path": "src/new.rs", "contents": "pub fn new() {}\n" }),
        json!({
            "bytes_written": 16,
            "worktreeChange": {
                "commit": "3f9c2ab",
                "files": { "created": 1, "updated": 0, "removed": 0 },
                "lines": { "added": 1, "removed": 0 }
            }
        }),
    )
    .await;

    // Then
    assert_eq!(
        the_tool_result(&outcome).worktree_change,
        Some(WorktreeChange {
            commit: Some("3f9c2ab".to_string()),
            files: FileCounts {
                created: 1,
                updated: 0,
                removed: 0
            },
            lines: LineCounts {
                added: 1,
                removed: 0
            },
        })
    );
}

#[tokio::test]
async fn a_mutating_call_that_changed_nothing_reports_counts_without_a_commit() {
    // Given / When
    let outcome = a_turn_where_the_model_calls(
        "SHELL",
        json!({ "command": "true" }),
        json!({
            "stdout": "",
            "stderr": "",
            "exit_code": 0,
            "worktreeChange": {
                "files": { "created": 0, "updated": 0, "removed": 0 },
                "lines": { "added": 0, "removed": 0 }
            }
        }),
    )
    .await;

    // Then
    assert_eq!(
        the_tool_result(&outcome).worktree_change,
        Some(WorktreeChange::default())
    );
}

#[tokio::test]
async fn the_worktree_change_is_serialized_beside_the_result_summary() {
    // Given
    let outcome = a_turn_where_the_model_calls(
        "WRITE",
        json!({ "path": "a.txt", "contents": "a\n" }),
        json!({
            "bytes_written": 2,
            "worktreeChange": {
                "commit": "9e01d4c",
                "files": { "created": 1, "updated": 0, "removed": 0 },
                "lines": { "added": 1, "removed": 0 }
            }
        }),
    )
    .await;

    // When
    let serialized = serde_json::to_value(the_tool_result(&outcome)).expect("serializes");

    // Then
    assert_eq!(
        serialized["worktreeChange"],
        json!({
            "commit": "9e01d4c",
            "files": { "created": 1, "updated": 0, "removed": 0 },
            "lines": { "added": 1, "removed": 0 }
        })
    );
}

/// Guards the unchanged path: a result without `worktreeChange` reports none.
#[tokio::test]
async fn a_read_reports_no_worktree_change() {
    // Given / When
    let outcome = a_turn_where_the_model_calls(
        "READ",
        json!({ "path": "src/lib.rs" }),
        json!({ "content": "fn a() {}", "truncated": false, "total_lines": 1 }),
    )
    .await;

    // Then
    assert_eq!(the_tool_result(&outcome).worktree_change, None);
}
