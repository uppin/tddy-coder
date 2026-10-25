//! A conversation's status row is told apart from the agent's own calls by its source, which every consumer of raw
//! activity receives — not by its tool name, which an agent's own tool could share.
//!
//! Backlog: docs/dev/todo/2026-10-25-conversation-status-row-is-durable-only.md

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_RUNNING};
use tddy_service::subagent_conversation_replay::{
    conversation_row_entry, CONVERSATION_ROW_SOURCE, CONVERSATION_ROW_TOOL_NAME,
};

/// A running record named like a status row, for conversation `conv-1`, from `source`.
fn a_record_named_like_a_status_row_from(source: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: "subagent-conversation:conv-1".to_string(),
        tool_name: CONVERSATION_ROW_TOOL_NAME.to_string(),
        input: serde_json::json!({ "sessionId": "conv-1" }),
        status: STATUS_RUNNING.to_string(),
        result: serde_json::json!({ "awaitingUser": true }),
        error_message: String::new(),
        started_unix_ms: 1_000,
        completed_unix_ms: 0,
        source: source.to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}

#[test]
fn a_record_from_the_daemons_status_source_is_a_status_row() {
    // When
    let entry = conversation_row_entry(&a_record_named_like_a_status_row_from(
        CONVERSATION_ROW_SOURCE,
    ));

    // Then
    assert!(entry.is_some(), "the daemon's status row is read as one");
}

#[test]
fn an_agents_own_call_named_like_a_status_row_is_not_read_as_one() {
    // When
    let entry = conversation_row_entry(&a_record_named_like_a_status_row_from("claude-cli"));

    // Then
    assert!(
        entry.is_none(),
        "an agent's own `subagent_conversation` call is not the daemon's status row"
    );
}
