//! A subagent conversation's own status row decides its entry's status while the conversation is running: a
//! conversation waiting on the user reads as pending whatever the main agent's calls say. Once the conversation
//! has completed, the newest record by time decides, so a later call of the main agent wins.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING};
use tddy_service::acp_replay::frame_for_agent_activity;
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, ToolCallStatus,
};
use tddy_service::subagent_conversation_replay::{
    fold_subagent_calls, CONVERSATION_ROW_SOURCE, SUBAGENT_TOOL_CALL_PREFIX,
};

/// The conversation's own status row, as the daemon writes it: one call id per conversation.
fn a_conversation_row(status: &str, awaiting_user: bool, at: u64) -> AcpAgentMessage {
    frame_for_agent_activity(&AgentActivityRecord {
        call_id: "subagent-conversation:conv-1".to_string(),
        tool_name: "subagent_conversation".to_string(),
        input: serde_json::json!({ "sessionId": "conv-1" }),
        status: status.to_string(),
        result: if awaiting_user {
            serde_json::json!({ "awaitingUser": true })
        } else {
            serde_json::Value::Null
        },
        error_message: String::new(),
        started_unix_ms: at,
        completed_unix_ms: if status == STATUS_COMPLETED {
            at + 1
        } else {
            0
        },
        source: CONVERSATION_ROW_SOURCE.to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    })
}

/// A main-agent `subagent_*` call on conversation `conv-1`, with the given status.
fn a_main_agent_call(call_id: &str, tool: &str, status: &str, at: u64) -> AcpAgentMessage {
    frame_for_agent_activity(&AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: format!("mcp__tddy-tools__{tool}"),
        input: serde_json::json!({ "sessionId": "conv-1" }),
        status: status.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: at,
        completed_unix_ms: if status == STATUS_COMPLETED {
            at + 1
        } else {
            0
        },
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    })
}

/// The status of the one entry a fold leaves for `conv-1`.
fn folded_status(frames: Vec<AcpAgentMessage>) -> ToolCallStatus {
    let folded = fold_subagent_calls(frames);
    assert_eq!(
        folded.len(),
        1,
        "one entry for the conversation: {folded:?}"
    );
    let Some(acp_agent_message::Msg::SessionUpdate(notification)) = &folded[0].msg else {
        panic!("a session update: {:?}", folded[0]);
    };
    let Some(session_update::Update::ToolCall(call)) = notification
        .update
        .as_ref()
        .and_then(|update| update.update.as_ref())
    else {
        panic!("a tool call: {:?}", folded[0]);
    };
    assert_eq!(
        call.tool_call_id.as_ref().map(|id| id.value.as_str()),
        Some(format!("{SUBAGENT_TOOL_CALL_PREFIX}conv-1").as_str()),
        "the entry is the conversation's"
    );
    ToolCallStatus::try_from(call.status).expect("a status")
}

#[test]
fn a_running_conversation_awaiting_the_user_is_pending_even_after_a_later_completed_resume() {
    // Given the conversation is waiting on the user, and the resume call that deferred the request has completed
    let frames = vec![
        a_conversation_row(STATUS_RUNNING, true, 1_000),
        a_main_agent_call("c1", "subagent_resume", STATUS_COMPLETED, 2_000),
    ];

    // Then the entry reads as waiting for the user
    assert_eq!(folded_status(frames), ToolCallStatus::Pending);
}

#[test]
fn a_running_conversation_not_awaiting_the_user_is_in_progress_even_after_a_later_completed_call() {
    // Given the conversation is running, not waiting on the user, and a later call of the main agent completed
    let frames = vec![
        a_conversation_row(STATUS_RUNNING, false, 1_000),
        a_main_agent_call("c1", "subagent_prompt", STATUS_COMPLETED, 2_000),
    ];

    // Then the entry is in progress
    assert_eq!(folded_status(frames), ToolCallStatus::InProgress);
}

#[test]
fn a_completed_conversation_followed_by_a_newer_running_call_folds_to_that_call() {
    // Given the conversation completed, and the main agent then made a call that is still running
    let frames = vec![
        a_conversation_row(STATUS_COMPLETED, false, 1_000),
        a_main_agent_call("c1", "subagent_prompt", STATUS_RUNNING, 3_000),
    ];

    // Then the newer call decides
    assert_eq!(folded_status(frames), ToolCallStatus::InProgress);
}

#[test]
fn a_completed_conversation_alone_folds_to_completed() {
    // Given a conversation whose status row says it completed, and no call of the main agent after it
    let frames = vec![a_conversation_row(STATUS_COMPLETED, false, 1_000)];

    // Then the entry is completed
    assert_eq!(folded_status(frames), ToolCallStatus::Completed);
}
