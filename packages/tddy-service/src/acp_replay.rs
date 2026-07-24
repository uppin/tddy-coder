//! Persisted, self-contained ACP transcript (`acp-transcript.jsonl`) and its replay reader.
//!
//! The session persists its **own** ACP-mapped conversation so the read-only transcript replay does
//! not depend on the agent-CLI-owned `conversation.jsonl`. Each line is one `AcpAgentMessage`
//! (`session_update` frame) stamped with a real wall-clock `timestamp_unix_ms`, written at event
//! time by the coder presenter seam (see `tddy-coder`). Because history and live are produced by the
//! same mapper (`crate::convert_acp` + a persistent `OutboundState`), a replayed transcript is what a
//! live viewer would have seen.
//!
//! This module owns:
//! - the persisted format (`serialize_frame` / `deserialize_frames`) + file I/O
//!   (`append_acp_frame` / `read_acp_transcript`), and
//! - the frame builders (`agent_text_frame`, `tool_use_frame`) that stamp the timestamp and, for a
//!   tool call, the enriched title + `raw_input` + `kind`.

use std::io;
use std::io::Write as _;
use std::path::Path;

use prost::Message as _;

use crate::convert_acp::agent_message_chunk;
use crate::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, SessionNotification, SessionUpdate,
    ToolCall, ToolCallId, ToolCallStatus, ToolKind,
};

/// Session-dir filename of the persisted ACP transcript (sibling of `agent-activity.jsonl`).
pub const ACP_TRANSCRIPT_FILENAME: &str = "acp-transcript.jsonl";

/// Serialize one ACP frame to a single transcript line (no trailing newline).
///
/// prost types carry no serde derive, so the frame is encoded to its protobuf bytes and those
/// bytes are written as a JSON array of numbers — a lossless, self-describing line that
/// [`deserialize_frames`] can decode exactly.
pub fn serialize_frame(frame: &AcpAgentMessage) -> String {
    let bytes = frame.encode_to_vec();
    serde_json::to_string(&bytes).expect("Vec<u8> always serializes to JSON")
}

/// Deserialize transcript file contents (one frame per non-empty line) back into frames, in order.
///
/// Each line is the JSON byte array produced by [`serialize_frame`]; a line that fails to decode
/// is skipped so a single corrupt row never discards the rest of the transcript.
pub fn deserialize_frames(contents: &str) -> Vec<AcpAgentMessage> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let bytes: Vec<u8> = serde_json::from_str(line).ok()?;
            AcpAgentMessage::decode(&bytes[..]).ok()
        })
        .collect()
}

/// Append one frame to the session's `acp-transcript.jsonl` (creating it if absent).
pub fn append_acp_frame(session_dir: &Path, frame: &AcpAgentMessage) -> io::Result<()> {
    std::fs::create_dir_all(session_dir)?;
    let path = session_dir.join(ACP_TRANSCRIPT_FILENAME);
    let mut line = serialize_frame(frame);
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())
}

/// Read the persisted transcript and return its frames in write order.
///
/// Returns an empty `Vec` when the file does not exist (no transcript recorded yet).
pub fn read_acp_transcript(session_dir: &Path) -> io::Result<Vec<AcpAgentMessage>> {
    let path = session_dir.join(ACP_TRANSCRIPT_FILENAME);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(deserialize_frames(&contents))
}

/// Wrap a `SessionUpdate` in a `SessionNotification` frame stamped at `at_unix_ms`.
fn session_update_frame(update: SessionUpdate, at_unix_ms: i64) -> AcpAgentMessage {
    AcpAgentMessage {
        id: 0,
        msg: Some(acp_agent_message::Msg::SessionUpdate(SessionNotification {
            session_id: None,
            update: Some(update),
            timestamp_unix_ms: at_unix_ms,
        })),
    }
}

/// Build an agent-text transcript frame (an `agent_message_chunk`) stamped at `at_unix_ms`.
pub fn agent_text_frame(text: &str, at_unix_ms: i64) -> AcpAgentMessage {
    session_update_frame(agent_message_chunk(text.to_string()), at_unix_ms)
}

/// Map a tool name to its ACP [`ToolKind`], mirroring the categories the web renders.
fn tool_kind_for(tool_name: &str) -> ToolKind {
    match tool_name {
        "Read" => ToolKind::Read,
        "Write" | "Edit" => ToolKind::Edit,
        "Bash" => ToolKind::Execute,
        "Glob" | "Grep" | "ToolSearch" => ToolKind::Search,
        "Agent" => ToolKind::Think,
        _ => ToolKind::Other,
    }
}

/// Build an enriched tool-call transcript frame stamped at `at_unix_ms`: `title` is
/// `"<ToolName> <detail>"` (detail from the tool input, e.g. `main.rs L10-49`), `kind` is derived
/// from the tool name, and `raw_input` carries the full tool input as JSON.
pub fn tool_use_frame(
    id: u64,
    tool_name: &str,
    input: &serde_json::Value,
    status: ToolCallStatus,
    at_unix_ms: i64,
) -> AcpAgentMessage {
    let title = match tddy_core::stream::claude::tool_use_detail(tool_name, input) {
        Some(detail) => format!("{tool_name} {detail}"),
        None => tool_name.to_string(),
    };
    let update = SessionUpdate {
        update: Some(session_update::Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId {
                value: format!("tool-{id}"),
            }),
            title,
            kind: tool_kind_for(tool_name) as i32,
            status: status as i32,
            raw_input: serde_json::to_string(input).ok(),
            ..Default::default()
        })),
    };
    session_update_frame(update, at_unix_ms)
}

/// Build an enriched `tool_call` transcript frame from a persisted [`AgentActivityRecord`].
///
/// This is the agent-activity analogue of [`tool_use_frame`]: the frame carries the record's own
/// `call_id` (not a synthetic `tool-{n}` id), an enriched `title` (`"<ToolName> <detail>"`, or the
/// bare tool name when the input yields no detail), the [`ToolKind`] derived from the tool name, the
/// [`ToolCallStatus`] mapped from the record's wire status, and the full input as `raw_input` JSON.
/// It is stamped with the record's terminal timestamp when finished, else its start timestamp.
pub fn frame_for_agent_activity(
    record: &tddy_core::agent_activity::AgentActivityRecord,
) -> AcpAgentMessage {
    use tddy_core::agent_activity::{STATUS_COMPLETED, STATUS_ERROR};

    let title = match tddy_core::stream::claude::tool_use_detail(&record.tool_name, &record.input) {
        Some(detail) => format!("{} {detail}", record.tool_name),
        None => record.tool_name.clone(),
    };
    let status = match record.status.as_str() {
        STATUS_COMPLETED => ToolCallStatus::Completed,
        STATUS_ERROR => ToolCallStatus::Failed,
        // `running` (and any not-yet-terminal state) maps to in-progress.
        _ => ToolCallStatus::InProgress,
    };
    let at_unix_ms = if record.completed_unix_ms > 0 {
        record.completed_unix_ms
    } else {
        record.started_unix_ms
    } as i64;
    let update = SessionUpdate {
        update: Some(session_update::Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId {
                value: record.call_id.clone(),
            }),
            title,
            kind: tool_kind_for(&record.tool_name) as i32,
            status: status as i32,
            raw_input: serde_json::to_string(&record.input).ok(),
            ..Default::default()
        })),
    };
    session_update_frame(update, at_unix_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::acp::{acp_agent_message, content_block, session_update, ToolCall, ToolKind};
    use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED};

    /// The (text, timestamp) of an `agent_message_chunk` frame (panics on any other shape).
    fn agent_chunk(frame: &AcpAgentMessage) -> (String, i64) {
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                let text = match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                    Some(session_update::Update::AgentMessageChunk(c)) => {
                        match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                            Some(content_block::Block::Text(t)) => t.text.clone(),
                            other => panic!("expected text content, got {other:?}"),
                        }
                    }
                    other => panic!("expected AgentMessageChunk, got {other:?}"),
                };
                (text, n.timestamp_unix_ms)
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// The (ToolCall, timestamp) of a `tool_call` frame (panics on any other shape).
    fn tool_call(frame: &AcpAgentMessage) -> (ToolCall, i64) {
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => (tc, n.timestamp_unix_ms),
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    fn a_read_input() -> serde_json::Value {
        serde_json::json!({ "file_path": "src/main.rs", "offset": 10, "limit": 40 })
    }

    #[test]
    fn an_agent_text_frame_carries_the_text_and_its_timestamp() {
        // When
        let frame = agent_text_frame("Analyzing the parser.", 1_000);

        // Then
        assert_eq!(
            agent_chunk(&frame),
            ("Analyzing the parser.".to_string(), 1_000)
        );
    }

    #[test]
    fn a_read_tool_frame_is_labelled_with_its_file_and_line_range() {
        // When
        let frame = tool_use_frame(1, "Read", &a_read_input(), ToolCallStatus::Completed, 3_000);

        // Then — enriched title, tool kind, and timestamp
        let (tc, at) = tool_call(&frame);
        assert_eq!(tc.title, "Read main.rs L10-49");
        assert_eq!(tc.kind, ToolKind::Read as i32);
        assert_eq!(tc.status, ToolCallStatus::Completed as i32);
        assert_eq!(at, 3_000);
    }

    #[test]
    fn a_tool_frame_carries_the_full_input_as_raw_input_json() {
        // When
        let frame = tool_use_frame(1, "Read", &a_read_input(), ToolCallStatus::Completed, 3_000);

        // Then — the whole input round-trips through raw_input, so the web can render detail
        let (tc, _) = tool_call(&frame);
        let raw = tc
            .raw_input
            .expect("raw_input should carry the full tool input");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("raw_input is JSON");
        assert_eq!(parsed, a_read_input());
    }

    fn a_completed_read_record() -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: "call-read-1".to_string(),
            tool_name: "Read".to_string(),
            input: a_read_input(),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "content": "fn main() {}" }),
            error_message: String::new(),
            started_unix_ms: 2_000,
            completed_unix_ms: 3_000,
            source: "coder".to_string(),
        }
    }

    #[test]
    fn a_completed_agent_activity_record_becomes_an_enriched_tool_frame() {
        // When — a completed Read activity record is mapped to a transcript frame
        let frame = frame_for_agent_activity(&a_completed_read_record());

        // Then — the frame carries the record's own call_id, enriched title, kind, terminal status,
        // terminal timestamp, and the full input as raw_input JSON.
        let (tc, at) = tool_call(&frame);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-read-1");
        assert_eq!(tc.title, "Read main.rs L10-49");
        assert_eq!(tc.kind, ToolKind::Read as i32);
        assert_eq!(tc.status, ToolCallStatus::Completed as i32);
        assert_eq!(at, 3_000);
        let raw = tc
            .raw_input
            .expect("raw_input should carry the full tool input");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("raw_input is JSON");
        assert_eq!(parsed, a_read_input());
    }

    #[test]
    fn serializing_then_deserializing_the_transcript_round_trips_frames_in_order() {
        // Given — an agent turn followed by a tool call
        let first = agent_text_frame("Let me read the file.", 1_000);
        let second = tool_use_frame(1, "Read", &a_read_input(), ToolCallStatus::Completed, 3_000);
        let contents = format!(
            "{}\n{}\n",
            serialize_frame(&first),
            serialize_frame(&second)
        );

        // When
        let frames = deserialize_frames(&contents);

        // Then — both frames survive, in write order
        assert_eq!(frames, vec![first, second]);
    }
}
