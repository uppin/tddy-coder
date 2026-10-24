//! A subagent conversation's own transcript, and the one entry the session's transcript shows for
//! it.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING};
use tddy_service::acp_replay::{agent_text_frame, frame_for_agent_activity, tool_call_id_of};
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, ToolCallStatus,
};
use tddy_service::subagent_conversation_replay::{
    append_conversation_frame, fold_subagent_calls, read_conversation_transcript,
    SUBAGENT_TOOL_CALL_PREFIX,
};

/// The main agent's call of `tool` (an `mcp__tddy-tools__subagent_*` name) on `conversation`.
fn a_subagent_call(
    call_id: &str,
    tool: &str,
    conversation: &str,
    status: &str,
    at: u64,
) -> AcpAgentMessage {
    frame_for_agent_activity(&AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: format!("mcp__tddy-tools__{tool}"),
        input: serde_json::json!({ "sessionId": conversation, "prompt": "Find the parser" }),
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

fn a_read_call(call_id: &str, at: u64) -> AcpAgentMessage {
    frame_for_agent_activity(&AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: "Read".to_string(),
        input: serde_json::json!({ "file_path": "src/lib.rs" }),
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::json!({ "content": "" }),
        error_message: String::new(),
        started_unix_ms: at,
        completed_unix_ms: at + 1,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    })
}

fn tool_call_ids(frames: &[AcpAgentMessage]) -> Vec<String> {
    frames
        .iter()
        .filter_map(tool_call_id_of)
        .map(str::to_string)
        .collect()
}

fn status_of(frame: &AcpAgentMessage) -> ToolCallStatus {
    let Some(acp_agent_message::Msg::SessionUpdate(notification)) = &frame.msg else {
        panic!("a session update: {frame:?}");
    };
    let Some(session_update::Update::ToolCall(call)) = notification
        .update
        .as_ref()
        .and_then(|update| update.update.as_ref())
    else {
        panic!("a tool call: {frame:?}");
    };
    ToolCallStatus::try_from(call.status).expect("a status")
}

#[test]
fn subagent_calls_for_one_conversation_fold_into_one_tool_call() {
    // Given
    let frames = vec![
        a_subagent_call(
            "c1",
            "subagent_new_session",
            "conv-1",
            STATUS_COMPLETED,
            1_000,
        ),
        a_read_call("r1", 1_500),
        a_subagent_call("c2", "subagent_prompt", "conv-1", STATUS_COMPLETED, 2_000),
        a_subagent_call("c3", "subagent_await", "conv-1", STATUS_COMPLETED, 3_000),
    ];

    // When
    let folded = fold_subagent_calls(frames);

    // Then
    assert_eq!(
        tool_call_ids(&folded),
        [
            format!("{SUBAGENT_TOOL_CALL_PREFIX}conv-1"),
            "r1".to_string()
        ]
    );
}

#[test]
fn the_folded_entry_status_follows_the_last_call() {
    // Given
    let frames = vec![
        a_subagent_call(
            "c1",
            "subagent_new_session",
            "conv-1",
            STATUS_COMPLETED,
            1_000,
        ),
        a_subagent_call("c2", "subagent_prompt", "conv-1", STATUS_RUNNING, 2_000),
    ];

    // When
    let folded = fold_subagent_calls(frames);

    // Then
    assert_eq!(folded.len(), 1, "{folded:?}");
    assert_eq!(status_of(&folded[0]), ToolCallStatus::InProgress);
}

#[test]
fn subagent_calls_for_two_conversations_are_two_entries() {
    // Given
    let frames = vec![
        a_subagent_call("c1", "subagent_prompt", "conv-1", STATUS_COMPLETED, 1_000),
        a_subagent_call("c2", "subagent_prompt", "conv-2", STATUS_COMPLETED, 2_000),
        a_subagent_call("c3", "subagent_await", "conv-1", STATUS_COMPLETED, 3_000),
    ];

    // When
    let folded = fold_subagent_calls(frames);

    // Then
    assert_eq!(
        tool_call_ids(&folded),
        [
            format!("{SUBAGENT_TOOL_CALL_PREFIX}conv-1"),
            format!("{SUBAGENT_TOOL_CALL_PREFIX}conv-2"),
        ]
    );
}

#[test]
fn a_conversation_transcript_reads_back_what_was_appended_in_order() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
    append_conversation_frame(
        session_dir.path(),
        "conv-1",
        &agent_text_frame("Reading.", 1_000),
    )
    .expect("append");
    append_conversation_frame(
        session_dir.path(),
        "conv-1",
        &agent_text_frame("Done.", 2_000),
    )
    .expect("append");

    // When
    let transcript = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("the conversation recorded something");

    // Then
    assert_eq!(
        transcript,
        [
            agent_text_frame("Reading.", 1_000),
            agent_text_frame("Done.", 2_000)
        ]
    );
}

#[test]
fn a_conversation_that_recorded_nothing_reads_as_not_recorded() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");

    // When
    let transcript = read_conversation_transcript(session_dir.path(), "conv-never").expect("read");

    // Then
    assert_eq!(transcript, None);
}
