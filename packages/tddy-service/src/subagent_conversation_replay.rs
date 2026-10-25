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

use tddy_core::agent_activity::{
    AgentActivityRecord, STATUS_COMPLETED, STATUS_ERROR, STATUS_RUNNING,
};

use crate::acp_replay::{append_acp_frame, read_session_transcript, tool_call_id_of};
use crate::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, ToolCall, ToolCallId, ToolCallStatus,
};

/// The `tool_call_id` prefix of a session transcript's subagent-conversation entry.
pub const SUBAGENT_TOOL_CALL_PREFIX: &str = "subagent:";

/// The main agent's MCP tool names whose calls fold into a conversation's entry.
pub const SUBAGENT_TOOL_NAME_PREFIX: &str = "mcp__tddy-tools__subagent_";

/// The `result` key a running call carries while its turn waits on a permission request held for the user.
pub const AWAITING_USER: &str = "awaitingUser";

/// The tool name of a conversation's own status row: the record the daemon writes for a turn that is waiting on
/// the user, which no call of the main agent makes. One row per conversation, under [`conversation_row_call_id`].
pub const CONVERSATION_ROW_TOOL_NAME: &str = "subagent_conversation";

/// The `source` a conversation's status row carries: what marks it as the daemon's status row rather than a call of
/// the agent's, for every consumer of raw activity — the tool name alone is a name an agent's own tool could have.
pub const CONVERSATION_ROW_SOURCE: &str = "subagent-status";

/// The `call_id` prefix of a conversation's status row. Its frame is folded into the conversation's entry, but
/// carries its own id, so a row is told apart from the main agent's calls.
pub const CONVERSATION_ROW_CALL_PREFIX: &str = "subagent-conversation:";

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

/// Fold the subagent-conversation entries of a transcript into one entry per conversation, in the place of the
/// first frame that named it. Every other frame is kept as it was, in order.
///
/// While a conversation's own status row is running, the row decides the entry's status: a conversation waiting
/// on the user reads as pending whatever the main agent's calls say. Once the row has completed, the newest record
/// by time decides, so a call the main agent made after the conversation ended wins. The title is the one the
/// conversation's calls name between them, and the entry takes the conversation's id.
pub fn fold_subagent_calls(frames: Vec<AcpAgentMessage>) -> Vec<AcpAgentMessage> {
    let titles = conversation_titles(&frames);
    let mut last_call: HashMap<String, AcpAgentMessage> = HashMap::new();
    let mut last_row: HashMap<String, AcpAgentMessage> = HashMap::new();
    for frame in &frames {
        let Some(entry_id) = entry_id_of(frame) else {
            continue;
        };
        if is_conversation_row(frame) {
            last_row.insert(entry_id, frame.clone());
        } else {
            last_call.insert(entry_id, frame.clone());
        }
    }
    let mut emitted: HashSet<String> = HashSet::new();
    let mut folded = Vec::with_capacity(frames.len());
    for frame in frames {
        match entry_id_of(&frame) {
            Some(entry_id) => {
                if emitted.insert(entry_id.clone()) {
                    let call = last_call.get(&entry_id);
                    let decisive = decisive_state(&frame, call, last_row.get(&entry_id)).clone();
                    let title = titles
                        .get(&entry_id)
                        .map(String::as_str)
                        .unwrap_or_default();
                    folded.push(with_state_of(frame, &entry_id, &decisive, call, title));
                }
            }
            None => folded.push(frame),
        }
    }
    folded
}

/// The entry a frame belongs to: a call's own entry id, or the entry of the conversation its status row names.
fn entry_id_of(frame: &AcpAgentMessage) -> Option<String> {
    let id = tool_call_id_of(frame)?;
    if id.starts_with(SUBAGENT_TOOL_CALL_PREFIX) {
        return Some(id.to_string());
    }
    let conversation = id.strip_prefix(CONVERSATION_ROW_CALL_PREFIX)?;
    Some(format!("{SUBAGENT_TOOL_CALL_PREFIX}{conversation}"))
}

/// Whether a frame is a conversation's own status row rather than one of the main agent's calls.
fn is_conversation_row(frame: &AcpAgentMessage) -> bool {
    tool_call_id_of(frame).is_some_and(|id| id.starts_with(CONVERSATION_ROW_CALL_PREFIX))
}

/// The frame whose state an entry shows: the conversation's status row while the row is running, else whichever of
/// the conversation's last call and its row is newer by time, the call on a tie. `first` is the frame that names the
/// entry, which is one of the two, so it is the answer only when neither is given.
fn decisive_state<'a>(
    first: &'a AcpAgentMessage,
    call: Option<&'a AcpAgentMessage>,
    row: Option<&'a AcpAgentMessage>,
) -> &'a AcpAgentMessage {
    match (call, row) {
        (None, None) => first,
        (Some(call), None) => call,
        (None, Some(row)) => row,
        (Some(call), Some(row)) => {
            if is_running(row) || timestamp_of(row) > timestamp_of(call) {
                row
            } else {
                call
            }
        }
    }
}

/// The title of every conversation entry in `frames`, from the arguments of all the calls that name it.
/// The agent is named by the first call that names one, and the prompt is carried by a later call, so
/// the title needs the whole conversation rather than the first frame alone.
fn conversation_titles(frames: &[AcpAgentMessage]) -> HashMap<String, String> {
    let mut arguments: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for frame in frames {
        if let Some(entry_id) = entry_id_of(frame) {
            arguments
                .entry(entry_id)
                .or_default()
                .extend(raw_input_of(frame));
        }
    }
    arguments
        .into_iter()
        .map(|(entry_id, calls)| {
            let title = conversation_title(conversation_of(&entry_id), &calls);
            (entry_id, title)
        })
        .collect()
}

/// The conversation an entry id names: the id less [`SUBAGENT_TOOL_CALL_PREFIX`].
fn conversation_of(entry_id: &str) -> &str {
    entry_id
        .strip_prefix(SUBAGENT_TOOL_CALL_PREFIX)
        .unwrap_or(entry_id)
}

/// The tool call a frame carries, when it carries one.
fn tool_call_of(frame: &AcpAgentMessage) -> Option<&ToolCall> {
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(note)) => {
            match note
                .update
                .as_ref()
                .and_then(|update| update.update.as_ref())
            {
                Some(session_update::Update::ToolCall(call)) => Some(call),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The status a frame's tool call carries, when it carries one.
fn status_of(frame: &AcpAgentMessage) -> Option<ToolCallStatus> {
    tool_call_of(frame).and_then(|call| ToolCallStatus::try_from(call.status).ok())
}

/// Whether a frame's tool call is still running: pending on the user, or in progress.
fn is_running(frame: &AcpAgentMessage) -> bool {
    matches!(
        status_of(frame),
        Some(ToolCallStatus::Pending | ToolCallStatus::InProgress)
    )
}

/// The stamp a frame was recorded at; `0` for a frame that carries none.
fn timestamp_of(frame: &AcpAgentMessage) -> i64 {
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(note)) => note.timestamp_unix_ms,
        _ => 0,
    }
}

/// The arguments a tool-call frame carries, as JSON, when it carries any.
fn raw_input_of(frame: &AcpAgentMessage) -> Option<serde_json::Value> {
    serde_json::from_str(tool_call_of(frame)?.raw_input.as_deref()?).ok()
}

/// `first`, named for the whole conversation under `entry_id` and in the state `decisive` leaves it in, with the
/// output of the conversation's last call: the entry a conversation's first frame opened.
fn with_state_of(
    mut first: AcpAgentMessage,
    entry_id: &str,
    decisive: &AcpAgentMessage,
    last_call: Option<&AcpAgentMessage>,
    title: &str,
) -> AcpAgentMessage {
    let stamp = timestamp_of(decisive);
    let status = tool_call_of(decisive).map(|call| call.status);
    let output = last_call
        .and_then(tool_call_of)
        .and_then(|call| call.raw_output.clone());
    if let Some(acp_agent_message::Msg::SessionUpdate(note)) = first.msg.as_mut() {
        note.timestamp_unix_ms = stamp;
        if let Some(session_update::Update::ToolCall(call)) = note
            .update
            .as_mut()
            .and_then(|update| update.update.as_mut())
        {
            call.tool_call_id = Some(ToolCallId {
                value: entry_id.to_string(),
            });
            call.title = title.to_string();
            if let Some(status) = status {
                call.status = status;
            }
            call.raw_output = output;
        }
    }
    first
}

/// What one main-agent `subagent_*` call contributes to its conversation's entry.
pub struct SubagentEntry {
    /// `SUBAGENT_TOOL_CALL_PREFIX` + the conversation id for a call; the row's call id for a status row.
    pub tool_call_id: String,
    /// `"<agent> · <first prompt>"`, from the parts this call names. The fold replaces it with the title
    /// of the whole conversation, since one call may name only the agent or only the prompt.
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
        title: conversation_title(&conversation_id, std::slice::from_ref(&record.input)),
        status: entry_status(record),
    })
}

/// The `call_id` of `conversation_id`'s status row: one per conversation, so the daemon's rows for it coalesce.
pub fn conversation_row_call_id(conversation_id: &str) -> String {
    format!("{CONVERSATION_ROW_CALL_PREFIX}{conversation_id}")
}

/// The entry a conversation's own status row stands for. Its frame carries the row's call id, which the fold maps
/// into the conversation's entry, and its status is the row's own. `None` for any record that is not a status row.
pub fn conversation_row_entry(record: &AgentActivityRecord) -> Option<SubagentEntry> {
    if record.tool_name != CONVERSATION_ROW_TOOL_NAME {
        return None;
    }
    let conversation_id = conversation_named_by(record)?;
    Some(SubagentEntry {
        tool_call_id: conversation_row_call_id(&conversation_id),
        title: conversation_title(&conversation_id, std::slice::from_ref(&record.input)),
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

/// `"<agent> · <first prompt>"` from the parts the calls name, or the conversation id when they name
/// neither. The agent is the id a `subagent_new_session` call named it by, because an activity record
/// carries no label: the label lives on the roster row. The prompt is the first call's that carries one.
fn conversation_title(conversation_id: &str, inputs: &[serde_json::Value]) -> String {
    let agent = inputs.iter().find_map(named_agent);
    let prompt = inputs
        .iter()
        .find_map(|input| prompt_summary(input.get("prompt")));
    let parts: Vec<String> = [agent, prompt].into_iter().flatten().collect();
    if parts.is_empty() {
        conversation_id.to_string()
    } else {
        parts.join(" · ")
    }
}

/// The agent a call names, trimmed, unless it names none or a blank one.
fn named_agent(input: &serde_json::Value) -> Option<String> {
    input
        .get("agent")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
        .map(str::to_string)
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

/// The entry's status follows the call. A call whose turn is waiting on a permission request held for the
/// user is pending: the user is the one it waits on. A turn that paused on a request the main agent keeps is
/// still in progress, since the main agent is the one answering it. A completed call is completed and a failed
/// one failed.
fn entry_status(record: &AgentActivityRecord) -> ToolCallStatus {
    match record.status.as_str() {
        STATUS_COMPLETED
            if record
                .result
                .get("stopReason")
                .and_then(serde_json::Value::as_str)
                == Some(PERMISSION_REQUESTED) =>
        {
            ToolCallStatus::InProgress
        }
        STATUS_COMPLETED => ToolCallStatus::Completed,
        STATUS_ERROR => ToolCallStatus::Failed,
        STATUS_RUNNING
            if record
                .result
                .get(AWAITING_USER)
                .and_then(serde_json::Value::as_bool)
                == Some(true) =>
        {
            ToolCallStatus::Pending
        }
        // `running` (and any not-yet-terminal state) maps to in-progress.
        _ => ToolCallStatus::InProgress,
    }
}
