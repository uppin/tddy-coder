//! A subagent conversation's own replayable transcript, and the one entry the session's transcript
//! shows for it.
//!
//! Every conversation the daemon runs records its turns as ACP frames under the session it belongs
//! to, one transcript per conversation, read back through the same replay RPCs with a
//! `conversation_id`. In the session's own transcript, the main agent's `mcp__tddy-tools__subagent_*`
//! calls for one conversation fold into **one** tool-call entry whose id is
//! [`SUBAGENT_TOOL_CALL_PREFIX`] + the conversation id — the activity pane's subagent bubble.
//! [`subagent_entry_of`] makes that entry from one call, and [`fold_subagent_calls`] collapses the
//! entries of one conversation into one.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED, STATUS_ERROR};

use crate::acp_replay::{append_acp_frame, read_session_transcript, tool_call_id_of};
use crate::proto::acp::{acp_agent_message, session_update, AcpAgentMessage, ToolCallStatus};

/// The `tool_call_id` prefix of a session transcript's subagent-conversation entry.
pub const SUBAGENT_TOOL_CALL_PREFIX: &str = "subagent:";

/// The main agent's MCP tool names whose calls fold into a conversation's entry.
pub const SUBAGENT_TOOL_NAME_PREFIX: &str = "mcp__tddy-tools__subagent_";

/// The `stopReason` a `subagent_*` call answers with when its turn paused on a permission request: the
/// serde spelling of `StopReason::PermissionRequested` in `tddy-discovery`.
const PERMISSION_REQUESTED: &str = "permission_requested";

/// The longest first line of a prompt an entry's title carries, in characters.
const PROMPT_SUMMARY_CHARS: usize = 80;

/// Where `conversation_id`'s transcript lives under `session_dir`.
pub fn conversation_transcript_dir(session_dir: &Path, conversation_id: &str) -> PathBuf {
    session_dir.join("conversations").join(conversation_id)
}

/// Append one frame to `conversation_id`'s transcript, creating it on the first frame.
pub fn append_conversation_frame(
    session_dir: &Path,
    conversation_id: &str,
    frame: &AcpAgentMessage,
) -> io::Result<()> {
    append_acp_frame(
        &conversation_transcript_dir(session_dir, conversation_id),
        frame,
    )
}

/// `conversation_id`'s recorded transcript, resolved the way a session's is
/// ([`read_session_transcript`]); `None` when that conversation recorded nothing (an in-jail loop, or
/// an id this session never ran).
pub fn read_conversation_transcript(
    session_dir: &Path,
    conversation_id: &str,
) -> io::Result<Option<Vec<AcpAgentMessage>>> {
    let frames =
        read_session_transcript(&conversation_transcript_dir(session_dir, conversation_id))?;
    Ok((!frames.is_empty()).then_some(frames))
}

/// Fold the subagent-conversation entries of a transcript into one entry per conversation, in the
/// place of the first frame that named it, carrying the status and stamp of the last. Every other
/// frame is kept as it was, in order.
pub fn fold_subagent_calls(frames: Vec<AcpAgentMessage>) -> Vec<AcpAgentMessage> {
    let latest: HashMap<String, AcpAgentMessage> = frames
        .iter()
        .filter_map(|frame| subagent_entry_id(frame).map(|id| (id.to_string(), frame.clone())))
        .collect();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut folded = Vec::with_capacity(frames.len());
    for frame in frames {
        match subagent_entry_id(&frame).map(str::to_string) {
            Some(id) => {
                if emitted.insert(id.clone()) {
                    folded.push(with_state_of(frame, &latest[&id]));
                }
            }
            None => folded.push(frame),
        }
    }
    folded
}

/// The conversation-entry id of a frame that is one, else `None`.
fn subagent_entry_id(frame: &AcpAgentMessage) -> Option<&str> {
    tool_call_id_of(frame).filter(|id| id.starts_with(SUBAGENT_TOOL_CALL_PREFIX))
}

/// `first`, carrying the status, output and stamp `last` left it in: the entry a conversation's first
/// call opened, in the state its last call put it.
fn with_state_of(mut first: AcpAgentMessage, last: &AcpAgentMessage) -> AcpAgentMessage {
    if let Some(acp_agent_message::Msg::SessionUpdate(note)) = first.msg.as_mut() {
        if let Some(acp_agent_message::Msg::SessionUpdate(last_note)) = &last.msg {
            note.timestamp_unix_ms = last_note.timestamp_unix_ms;
            if let (
                Some(session_update::Update::ToolCall(call)),
                Some(session_update::Update::ToolCall(last_call)),
            ) = (
                note.update
                    .as_mut()
                    .and_then(|update| update.update.as_mut()),
                last_note
                    .update
                    .as_ref()
                    .and_then(|update| update.update.as_ref()),
            ) {
                call.status = last_call.status;
                call.raw_output = last_call.raw_output.clone();
            }
        }
    }
    first
}

/// What one main-agent `subagent_*` call contributes to its conversation's entry.
pub struct SubagentEntry {
    /// `SUBAGENT_TOOL_CALL_PREFIX` + the conversation id.
    pub tool_call_id: String,
    /// `"<agent> · <first prompt>"`, from whichever of the two the call names.
    pub title: String,
    pub status: ToolCallStatus,
}

/// The conversation entry a main-agent `subagent_*` call stands for, or `None` when the call is not
/// one or names no conversation.
pub fn subagent_entry_of(record: &AgentActivityRecord) -> Option<SubagentEntry> {
    if !record.tool_name.starts_with(SUBAGENT_TOOL_NAME_PREFIX) {
        return None;
    }
    let conversation_id = conversation_named_by(record)?;
    Some(SubagentEntry {
        tool_call_id: format!("{SUBAGENT_TOOL_CALL_PREFIX}{conversation_id}"),
        title: entry_title(&conversation_id, &record.input),
        status: entry_status(record),
    })
}

/// The conversation a call names: the `sessionId` it was called with, else the one it answered with —
/// `subagent_new_session` mints its id, so the caller need not name it.
fn conversation_named_by(record: &AgentActivityRecord) -> Option<String> {
    [&record.input, &record.result]
        .into_iter()
        .find_map(|json| {
            json.get("sessionId")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())
        })
        .map(str::to_string)
}

/// `"<agent> · <first prompt>"` from the parts the call names, or the conversation id when it names
/// neither.
fn entry_title(conversation_id: &str, input: &serde_json::Value) -> String {
    let agent = input
        .get("agent")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|agent| !agent.is_empty());
    let prompt = prompt_summary(input.get("prompt"));
    let parts: Vec<&str> = [agent, prompt.as_deref()].into_iter().flatten().collect();
    if parts.is_empty() {
        conversation_id.to_string()
    } else {
        parts.join(" · ")
    }
}

/// The first non-blank line of a prompt, cut to [`PROMPT_SUMMARY_CHARS`]. A prompt is a bare string or
/// a list of content blocks, whose texts are joined.
fn prompt_summary(prompt: Option<&serde_json::Value>) -> Option<String> {
    let text = match prompt? {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(if line.chars().count() > PROMPT_SUMMARY_CHARS {
        let cut: String = line.chars().take(PROMPT_SUMMARY_CHARS).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    })
}

/// The entry's status follows the call: running is in progress, a turn paused on a permission request
/// is pending, a completed call is completed and a failed one failed.
fn entry_status(record: &AgentActivityRecord) -> ToolCallStatus {
    match record.status.as_str() {
        STATUS_COMPLETED
            if record
                .result
                .get("stopReason")
                .and_then(serde_json::Value::as_str)
                == Some(PERMISSION_REQUESTED) =>
        {
            ToolCallStatus::Pending
        }
        STATUS_COMPLETED => ToolCallStatus::Completed,
        STATUS_ERROR => ToolCallStatus::Failed,
        // `running` (and any not-yet-terminal state) maps to in-progress.
        _ => ToolCallStatus::InProgress,
    }
}
