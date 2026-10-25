//! A conversation's own status row: what the daemon records while a turn waits on the user.
//!
//! The main agent's `subagent_*` calls say what a conversation was asked. None of them says the conversation is
//! waiting on the user, because the call that deferred the request returns at once and the turn carries on in the
//! daemon. The daemon records that itself, as one activity row per conversation under a stable call id. The replay
//! folds the row into the conversation's entry (see `tddy_service::subagent_conversation_replay`).
//!
//! Written only for turns that hand a request to the user. A turn that does not, writes nothing here.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::io;
use std::path::Path;

use tddy_core::agent_activity::{
    append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
};
use tddy_service::subagent_conversation_replay::{
    conversation_row_call_id, CONVERSATION_ROW_TOOL_NAME,
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

/// Record `status` as `conversation_id`'s status row under `session_dir`.
///
/// The row names the conversation as `sessionId`, which is how the replay reads the conversation a row belongs to.
/// A failure is logged and not returned: the row is what a reader sees while a turn waits, and the turn itself is
/// not affected by it, so no caller treats it as fatal.
///
// TODO(web-approvals): the row is durable only. It is not published on the daemon's AgentActivityHub, so a live
// stream shows the waiting state on the next replay, not when it starts. The raw activity snapshot
// (`tddy-session-activity`'s `StreamAgentActivity`) also returns the row as a tool call named `subagent_conversation`,
// which a consumer of raw activity has to tell apart from the agent's own calls.
pub(crate) fn record(session_dir: &Path, conversation_id: &str, status: ConversationStatus) {
    if let Err(e) = append(session_dir, conversation_id, status) {
        log::warn!(
            target: "tddy_session_agents::conversation_status",
            "conversation '{conversation_id}' could not record its status {status:?}: {e}"
        );
    }
}

fn append(session_dir: &Path, conversation_id: &str, status: ConversationStatus) -> io::Result<()> {
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
    append_agent_activity(
        session_dir,
        &AgentActivityRecord {
            call_id: conversation_row_call_id(conversation_id),
            tool_name: CONVERSATION_ROW_TOOL_NAME.to_string(),
            input: serde_json::json!({ "sessionId": conversation_id }),
            status: status.to_string(),
            result,
            error_message: String::new(),
            started_unix_ms: now,
            completed_unix_ms,
            source: "coder".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        },
    )
}
