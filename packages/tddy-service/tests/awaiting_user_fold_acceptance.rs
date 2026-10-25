//! A subagent conversation's entry reads as waiting for the user only while a permission request is held for
//! the user. A request the main agent answers itself is not awaiting the user, and a finished turn is completed.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING};
use tddy_service::proto::acp::ToolCallStatus;
use tddy_service::subagent_conversation_replay::subagent_entry_of;

/// A main-agent `subagent_resume` call on conversation `conv-1`, with the given status and result.
fn a_resume_call(status: &str, result: serde_json::Value) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: "call-1".to_string(),
        tool_name: "mcp__tddy-tools__subagent_resume".to_string(),
        input: serde_json::json!({ "sessionId": "conv-1", "permission": { "requestId": "perm-1" } }),
        status: status.to_string(),
        result,
        error_message: String::new(),
        started_unix_ms: 0,
        completed_unix_ms: 0,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}

fn status_of(record: &AgentActivityRecord) -> ToolCallStatus {
    subagent_entry_of(record)
        .expect("a subagent call on a conversation has an entry")
        .status
}

#[test]
fn a_running_call_waiting_on_a_request_held_for_the_user_is_pending() {
    // Given
    let record = a_resume_call(STATUS_RUNNING, serde_json::json!({ "awaitingUser": true }));

    // Then
    assert_eq!(status_of(&record), ToolCallStatus::Pending);
}

#[test]
fn a_running_call_not_awaiting_the_user_is_in_progress() {
    // Given
    let record = a_resume_call(STATUS_RUNNING, serde_json::json!({ "awaitingUser": false }));

    // Then
    assert_eq!(status_of(&record), ToolCallStatus::InProgress);
}

#[test]
fn a_plain_running_call_is_in_progress() {
    // Given
    let record = a_resume_call(STATUS_RUNNING, serde_json::Value::Null);

    // Then
    assert_eq!(status_of(&record), ToolCallStatus::InProgress);
}

#[test]
fn a_completed_turn_that_paused_on_a_request_the_main_agent_keeps_is_in_progress() {
    // Given
    let record = a_resume_call(
        STATUS_COMPLETED,
        serde_json::json!({ "stopReason": "permission_requested" }),
    );

    // Then
    assert_eq!(status_of(&record), ToolCallStatus::InProgress);
}

#[test]
fn a_completed_turn_is_completed() {
    // Given
    let record = a_resume_call(
        STATUS_COMPLETED,
        serde_json::json!({ "stopReason": "end_turn" }),
    );

    // Then
    assert_eq!(status_of(&record), ToolCallStatus::Completed);
}
