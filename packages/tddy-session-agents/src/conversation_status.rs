//! A conversation's own status row: what the daemon records while a turn waits on the user.
//!
//! The main agent's `subagent_*` calls say what a conversation was asked. None of them says the conversation is
//! waiting on the user, because the call that deferred the request returns at once and the turn carries on in the
//! daemon. The daemon records that itself, as one activity row per conversation under a stable call id. The replay
//! folds the row into the conversation's entry (see `tddy_service::subagent_conversation_replay`).
//!
//! Written only for turns that hand a request to the user. A turn that does not, writes nothing here.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use std::path::Path;

use tddy_daemon_kernel::AgentActivityHub;

use tddy_core::agent_activity::{
    append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
};
use tddy_service::subagent_conversation_replay::{
    conversation_row_call_id, CONVERSATION_ROW_SOURCE, CONVERSATION_ROW_TOOL_NAME,
};

use crate::conversation_transcript::now_unix_ms;

/// What a conversation's status row says at the moment it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationStatus {
    /// Running, and waiting on a permission request the main agent handed to the user.
    AwaitingUser,
    /// Running, and no longer waiting on the user: the request was answered and the turn carries on.
    Running,
    /// The turn has ended.
    Completed,
}

/// Record `status` as `conversation_id`'s status row under `session_dir`, then publish it on `activity` for session
/// `session_id` — the path `ReportAgentActivity` takes — so a live subscriber sees it at once rather than at its next
/// snapshot.
///
/// The row names the conversation as `sessionId`, which is how the replay reads the conversation a row belongs to,
/// and carries [`CONVERSATION_ROW_SOURCE`], which is how any reader of raw activity tells it from an agent's call.
/// A failure to append is logged and not returned, and nothing is published for it: the row is what a reader sees
/// while a turn waits, and the turn itself is not affected by it, so no caller treats it as fatal.
pub(crate) fn record(
    activity: &AgentActivityHub,
    session_dir: &Path,
    session_id: &str,
    conversation_id: &str,
    status: ConversationStatus,
) {
    let row = status_row(conversation_id, status);
    match append_agent_activity(session_dir, &row) {
        Ok(()) => activity.publish(session_id, row),
        Err(e) => log::warn!(
            target: "tddy_session_agents::conversation_status",
            "conversation '{conversation_id}' could not record its status {status:?}: {e}"
        ),
    }
}

fn status_row(conversation_id: &str, status: ConversationStatus) -> AgentActivityRecord {
    let now = now_unix_ms() as u64;
    let (status, result, completed_unix_ms) = match status {
        ConversationStatus::AwaitingUser => (
            STATUS_RUNNING,
            serde_json::json!({ "awaitingUser": true }),
            0,
        ),
        ConversationStatus::Running => (STATUS_RUNNING, serde_json::Value::Null, 0),
        ConversationStatus::Completed => (STATUS_COMPLETED, serde_json::Value::Null, now),
    };
    AgentActivityRecord {
        call_id: conversation_row_call_id(conversation_id),
        tool_name: CONVERSATION_ROW_TOOL_NAME.to_string(),
        input: serde_json::json!({ "sessionId": conversation_id }),
        status: status.to_string(),
        result,
        error_message: String::new(),
        started_unix_ms: now,
        completed_unix_ms,
        source: CONVERSATION_ROW_SOURCE.to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}
