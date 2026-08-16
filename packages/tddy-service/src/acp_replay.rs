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
//!   (`append_acp_frame` / `read_acp_transcript`),
//! - the session's replayable transcript (`read_session_transcript`), which resolves this file
//!   *together with* the durable `agent-activity.jsonl` — the only store a daemon-hosted
//!   (claude-cli / sandbox) session writes, and
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

/// Read the session's **replayable transcript**, resolved from both persisted stores.
///
/// `acp-transcript.jsonl` is written by the coder presenter seam alone, so a session hosted
/// elsewhere (claude-cli / sandbox: the daemon records its tool calls) has no transcript file at all
/// — only the durable `agent-activity.jsonl`. Replaying one store leaves the other's history
/// unreachable, so this merges them:
/// - the persisted ACP frames ([`read_acp_transcript`]), plus
/// - the coalesced agent-activity calls ([`tddy_core::agent_activity::read_agent_activity`]) mapped
///   through [`frame_for_agent_activity`] — the same mapping the live tail uses;
/// - interleaved by recorded timestamp (each store contributing in its own recorded order), and
/// - deduped by `tool_call_id`, so a call recorded in both stores is replayed once, in the latest
///   state either store recorded for it.
///
/// A missing file on either side contributes nothing and is not an error.
pub fn read_session_transcript(session_dir: &Path) -> io::Result<Vec<AcpAgentMessage>> {
    let persisted_frames = read_acp_transcript(session_dir)?;
    let activity_frames: Vec<AcpAgentMessage> =
        tddy_core::agent_activity::read_agent_activity(session_dir)?
            .iter()
            .map(frame_for_agent_activity)
            .collect();
    Ok(latest_state_per_tool_call(merge_by_timestamp(
        persisted_frames,
        activity_frames,
    )))
}

/// The wall-clock stamp a frame was recorded at; `0` for a frame shape that carries no timestamp
/// (only `session_update` frames are ever persisted), which replays it as the oldest.
fn frame_timestamp(frame: &AcpAgentMessage) -> i64 {
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => n.timestamp_unix_ms,
        _ => 0,
    }
}

/// Interleave two already time-ordered frame lists into one, oldest first.
///
/// The merge is stable: each list keeps its own recorded order, and a tie resolves in favour of
/// `persisted` (the store that also carries agent text, so its narrative stays contiguous).
fn merge_by_timestamp(
    persisted: Vec<AcpAgentMessage>,
    activity: Vec<AcpAgentMessage>,
) -> Vec<AcpAgentMessage> {
    let mut merged = Vec::with_capacity(persisted.len() + activity.len());
    let mut persisted = persisted.into_iter().peekable();
    let mut activity = activity.into_iter().peekable();
    loop {
        let take_activity = match (persisted.peek(), activity.peek()) {
            (Some(p), Some(a)) => frame_timestamp(a) < frame_timestamp(p),
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => break,
        };
        let next = if take_activity {
            activity.next()
        } else {
            persisted.next()
        };
        merged.extend(next);
    }
    merged
}

/// Drop every superseded record of a tool call, keeping each `tool_call_id`'s **last** frame in
/// replay order — the latest state recorded for that call. Frames without a `tool_call_id` (agent
/// text) are all kept.
fn latest_state_per_tool_call(frames: Vec<AcpAgentMessage>) -> Vec<AcpAgentMessage> {
    let mut latest_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (index, frame) in frames.iter().enumerate() {
        if let Some(id) = tool_call_id_of(frame) {
            latest_index.insert(id.to_string(), index);
        }
    }
    frames
        .into_iter()
        .enumerate()
        .filter(|(index, frame)| match tool_call_id_of(frame) {
            Some(id) => latest_index.get(id) == Some(index),
            None => true,
        })
        .map(|(_, frame)| frame)
        .collect()
}

/// The `tool_call_id` of a `tool_call` frame, or `None` for any other frame shape.
///
/// Public because a live relay has to recognise the *second* record of a call it has already
/// numbered: a tool call broadcasts twice (its `running` then its terminal record) but coalesces
/// into a single transcript entry, so the refinement must reuse that entry's position rather than
/// take a new one. [`tool_call_ids`] answers only "which calls", not "at which position".
pub fn tool_call_id_of(frame: &AcpAgentMessage) -> Option<&str> {
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                Some(session_update::Update::ToolCall(tc)) => {
                    tc.tool_call_id.as_ref().map(|id| id.value.as_str())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// True for an `agent_message_chunk` (agent-text) frame.
fn is_agent_text(frame: &AcpAgentMessage) -> bool {
    matches!(
        &frame.msg,
        Some(acp_agent_message::Msg::SessionUpdate(n))
            if matches!(
                n.update.as_ref().and_then(|u| u.update.as_ref()),
                Some(session_update::Update::AgentMessageChunk(_))
            )
    )
}

/// The distinct tool-call ids present in a transcript, in no particular order.
///
/// Used to seed a live count relay's "already counted" set so a tool call whose `running` frame is
/// in the snapshot is not counted a second time when its terminal record arrives live.
pub fn tool_call_ids(frames: &[AcpAgentMessage]) -> std::collections::HashSet<String> {
    frames
        .iter()
        .filter_map(|f| tool_call_id_of(f).map(str::to_string))
        .collect()
}

/// Count the **coalesced** activity entries in a transcript: one per agent-text frame plus one per
/// distinct tool-call id.
///
/// A tool call persists a `running` then a terminal frame under the *same* `tool_call_id`, which the
/// reader (and the web pane) coalesce into a single row. This counts what the pane renders — the
/// number the Agent Activity badge reflects — not the raw frame count.
pub fn count_activity_entries(frames: &[AcpAgentMessage]) -> u64 {
    let mut seen_tool_ids = std::collections::HashSet::new();
    let mut count: u64 = 0;
    for frame in frames {
        match tool_call_id_of(frame) {
            Some(id) => {
                if seen_tool_ids.insert(id) {
                    count += 1;
                }
            }
            None if is_agent_text(frame) => count += 1,
            None => {}
        }
    }
    count
}

/// How many transcript frames a page carries when the caller does not ask for a size (the proto's
/// `page_size = 0`): comfortably more than a viewport of entries, two orders of magnitude below a
/// large session's transcript.
pub const DEFAULT_REPLAY_PAGE_SIZE: usize = 100;

/// One page of the resolved transcript, oldest-first within the page.
///
/// `first_seq` is the absolute 0-based position of `frames[0]` in the whole transcript — the reverse
/// cursor a reader pages backwards from. It is meaningless when `frames` is empty, which is why
/// `at_oldest` is carried explicitly: an empty page (the cursor already sits at the head) would
/// otherwise be indistinguishable from a one-frame page at the head.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptPage<'a> {
    pub first_seq: u64,
    pub frames: &'a [AcpAgentMessage],
    pub at_oldest: bool,
}

/// The newest `page_size` frames, oldest-first within the page — what a tail-first replay opens on.
///
/// `page_size == 0` falls back to [`DEFAULT_REPLAY_PAGE_SIZE`]. A transcript shorter than one page
/// tails to the whole transcript, reported `at_oldest`.
pub fn tail_page(frames: &[AcpAgentMessage], page_size: usize) -> TranscriptPage<'_> {
    // The newest page is exactly the page before the transcript's end.
    page_before(frames, frames.len() as u64, page_size)
}

/// The page of frames immediately older than `before_seq` (exclusive), oldest-first within the page.
///
/// `page_size == 0` falls back to [`DEFAULT_REPLAY_PAGE_SIZE`]. `before_seq == 0` yields an empty
/// `at_oldest` page — there is nothing older than the head. A `before_seq` beyond the transcript's
/// length clamps to that length, so a cursor held across a rewritten (shorter) transcript resolves
/// to the newest page rather than to nothing.
pub fn page_before(
    frames: &[AcpAgentMessage],
    before_seq: u64,
    page_size: usize,
) -> TranscriptPage<'_> {
    let page_size = if page_size == 0 {
        DEFAULT_REPLAY_PAGE_SIZE
    } else {
        page_size
    };
    // A cursor too wide for `usize` is still a cursor past the end, and clamps the same way.
    let end = usize::try_from(before_seq)
        .unwrap_or(usize::MAX)
        .min(frames.len());
    let start = end.saturating_sub(page_size);
    TranscriptPage {
        first_seq: start as u64,
        frames: &frames[start..end],
        at_oldest: start == 0,
    }
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
    // A completed call carries its result as `raw_output`; a still-running row has a JSON `null`
    // result and no output yet, so it stays `None`.
    let raw_output = if record.result.is_null() {
        None
    } else {
        serde_json::to_string(&record.result).ok()
    };
    let update = SessionUpdate {
        update: Some(session_update::Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId {
                value: record.call_id.clone(),
            }),
            title,
            kind: tool_kind_for(&record.tool_name) as i32,
            status: status as i32,
            raw_input: serde_json::to_string(&record.input).ok(),
            raw_output,
            ..Default::default()
        })),
    };
    session_update_frame(update, at_unix_ms)
}

/// A single tool call's stripped-out bodies: the exact JSON strings the transcript inlines. Either
/// may be absent — a still-running call carries `raw_input` but no `raw_output` yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolCallDetail {
    pub raw_input: Option<String>,
    pub raw_output: Option<String>,
}

/// Return `frame` with a tool-call's heavy bodies removed: `raw_input`/`raw_output` cleared,
/// `title`/`kind`/`status`/`tool_call_id` retained. Any non-tool-call frame (agent text) is returned
/// unchanged. This is the seam both replay hosts apply at their frame-wrap point so every streamed
/// `SNAPSHOT_THEN_LIVE` / `LIVE_ONLY` tool-call frame is body-less.
pub fn strip_tool_body(frame: &AcpAgentMessage) -> AcpAgentMessage {
    let mut frame = frame.clone();
    if let Some(acp_agent_message::Msg::SessionUpdate(n)) = frame.msg.as_mut() {
        if let Some(session_update::Update::ToolCall(tc)) =
            n.update.as_mut().and_then(|u| u.update.as_mut())
        {
            tc.raw_input = None;
            tc.raw_output = None;
        }
    }
    frame
}

/// Resolve one tool call's full bodies from the session's coalesced transcript.
///
/// Reads [`read_session_transcript`] (the same view the stream replays) and returns the
/// [`ToolCallDetail`] of the frame whose `tool_call_id` equals `tool_call_id`, or `None` when no
/// frame carries that id. Because it reads the identical deduped view, the id an operator clicked in
/// the stream resolves to the same call the stream showed (latest recorded state).
pub fn tool_call_detail(
    session_dir: &Path,
    tool_call_id: &str,
) -> io::Result<Option<ToolCallDetail>> {
    let frames = read_session_transcript(session_dir)?;
    for frame in &frames {
        if tool_call_id_of(frame) != Some(tool_call_id) {
            continue;
        }
        if let Some(acp_agent_message::Msg::SessionUpdate(n)) = &frame.msg {
            if let Some(session_update::Update::ToolCall(tc)) =
                n.update.as_ref().and_then(|u| u.update.as_ref())
            {
                return Ok(Some(ToolCallDetail {
                    raw_input: tc.raw_input.clone(),
                    raw_output: tc.raw_output.clone(),
                }));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::acp::{acp_agent_message, content_block, session_update, ToolCall, ToolKind};
    use tddy_core::agent_activity::{
        append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
    };

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
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
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
    fn a_completed_tool_frame_carries_the_full_result_as_raw_output_json() {
        // When — a completed tool call (with a result) is mapped for the persisted transcript
        let frame = frame_for_agent_activity(&a_completed_read_record());

        // Then — the whole result round-trips through raw_output, so the web detail dialog can render
        // the tool's output as prettified JSON alongside its input.
        let (tc, _) = tool_call(&frame);
        let raw = tc
            .raw_output
            .expect("raw_output should carry the full tool result");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("raw_output is JSON");
        assert_eq!(parsed, serde_json::json!({ "content": "fn main() {}" }));
    }

    #[test]
    fn count_activity_entries_coalesces_a_tool_calls_running_and_terminal_frames() {
        // Given — a transcript: one agent-text frame, the SAME tool call persisted as running then
        // completed (two frames, one id), then a second distinct tool call.
        fn record(call_id: &str, status: &str) -> AgentActivityRecord {
            AgentActivityRecord {
                call_id: call_id.to_string(),
                tool_name: "Read".to_string(),
                input: a_read_input(),
                status: status.to_string(),
                result: serde_json::Value::Null,
                error_message: String::new(),
                started_unix_ms: 1_000,
                completed_unix_ms: 0,
                source: "coder".to_string(),
                head_commit: String::new(),
                activity_seq: 0,
                changed_paths: Vec::new(),
            }
        }
        let frames = vec![
            agent_text_frame("Analyzing.", 1_000),
            frame_for_agent_activity(&record("call-a", "running")),
            frame_for_agent_activity(&record("call-a", STATUS_COMPLETED)),
            frame_for_agent_activity(&record("call-b", STATUS_COMPLETED)),
        ];

        // Then — the running+terminal pair for call-a coalesces: 1 text + 2 distinct calls = 3
        assert_eq!(count_activity_entries(&frames), 3);
    }

    // -----------------------------------------------------------------------
    // read_session_transcript — the session's replayable transcript, resolved from BOTH persisted
    // stores (`acp-transcript.jsonl` + the durable `agent-activity.jsonl`).
    //
    // Bug (fc990524): the replay snapshot reads only `acp-transcript.jsonl`, which is written by the
    // coder presenter seam alone. Every daemon-hosted (claude-cli / sandbox) session on disk has a
    // multi-MB `agent-activity.jsonl` and NO `acp-transcript.jsonl`, so the pane opens empty while
    // its badge counts live activity.
    // -----------------------------------------------------------------------

    /// The `tool_call_id` of every `tool_call` frame in a replayed transcript, in replay order.
    fn replayed_tool_call_ids(frames: &[AcpAgentMessage]) -> Vec<String> {
        frames
            .iter()
            .filter_map(|f| tool_call_id_of(f).map(str::to_string))
            .collect()
    }

    /// The `timestamp_unix_ms` of every frame in a replayed transcript, in replay order.
    fn replayed_timestamps(frames: &[AcpAgentMessage]) -> Vec<i64> {
        frames
            .iter()
            .map(|f| match &f.msg {
                Some(acp_agent_message::Msg::SessionUpdate(n)) => n.timestamp_unix_ms,
                other => panic!("expected a SessionUpdate frame, got {other:?}"),
            })
            .collect()
    }

    /// A completed Bash record, stamped after [`a_completed_read_record`]'s call.
    fn a_completed_bash_record() -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: "call-bash-2".to_string(),
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "cargo build" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "ok" }),
            error_message: String::new(),
            started_unix_ms: 4_000,
            completed_unix_ms: 5_000,
            source: "claude-cli".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        }
    }

    #[test]
    fn a_session_with_only_agent_activity_rows_replays_each_call_as_a_tool_call_frame() {
        // Given — a session dir holding ONLY the durable agent-activity log: the shape every
        // daemon-hosted session on disk has (nothing ever wrote `acp-transcript.jsonl` there)
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();
        append_agent_activity(dir.path(), &a_completed_bash_record()).unwrap();

        // When — the session's replayable transcript is resolved
        let frames = read_session_transcript(dir.path()).expect("read session transcript");

        // Then — both persisted calls are replayable, in recorded order
        assert_eq!(
            replayed_tool_call_ids(&frames),
            ["call-read-1", "call-bash-2"]
        );
    }

    #[test]
    fn a_tool_call_recorded_in_both_stores_is_replayed_once_in_its_latest_state() {
        // Given — the ACP transcript holds call-read-1 while it was still running, and the
        // agent-activity log holds the same call's terminal (completed) row
        let dir = tempfile::tempdir().unwrap();
        let running = AgentActivityRecord {
            status: STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            completed_unix_ms: 0,
            ..a_completed_read_record()
        };
        append_acp_frame(dir.path(), &frame_for_agent_activity(&running)).unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();

        // When — the session's replayable transcript is resolved
        let frames = read_session_transcript(dir.path()).expect("read session transcript");

        // Then — one entry for the call, carrying its latest (completed) state
        assert_eq!(replayed_tool_call_ids(&frames), ["call-read-1"]);
        assert_eq!(
            tool_call(&frames[0]).0.status,
            ToolCallStatus::Completed as i32
        );
    }

    #[test]
    fn frames_from_both_stores_are_replayed_in_recorded_time_order() {
        // Given — an agent-text frame recorded at 6_000 in the ACP transcript, and an older tool
        // call (terminal at 3_000) in the agent-activity log
        let dir = tempfile::tempdir().unwrap();
        append_acp_frame(dir.path(), &agent_text_frame("Done reading.", 6_000)).unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();

        // When — the session's replayable transcript is resolved
        let frames = read_session_transcript(dir.path()).expect("read session transcript");

        // Then — the two stores interleave by recorded wall-clock, oldest first
        assert_eq!(replayed_timestamps(&frames), [3_000, 6_000]);
    }

    #[test]
    fn counting_a_session_with_only_agent_activity_rows_matches_its_replayed_entries() {
        // Given — two calls persisted in the agent-activity log alone
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();
        append_agent_activity(dir.path(), &a_completed_bash_record()).unwrap();

        // When — the badge count is derived from the same resolved transcript the pane replays
        let frames = read_session_transcript(dir.path()).expect("read session transcript");

        // Then — the count matches what opening the pane will show (2), so a badge never promises
        // entries the transcript cannot deliver
        assert_eq!(count_activity_entries(&frames), 2);
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

    // -----------------------------------------------------------------------
    // strip_tool_body — the seam both replay hosts apply so streamed tool-call frames carry only
    // metadata (title/status/kind/id), never the heavy raw_input/raw_output bodies.
    // -----------------------------------------------------------------------

    #[test]
    fn stripping_a_tool_call_frame_drops_its_raw_input_and_raw_output() {
        // Given — a completed tool-call frame carrying a full raw_input and raw_output
        let frame = frame_for_agent_activity(&a_completed_read_record());

        // When — the body is stripped
        let stripped = strip_tool_body(&frame);

        // Then — both bodies are gone
        let (tc, _) = tool_call(&stripped);
        assert_eq!(tc.raw_input, None);
        assert_eq!(tc.raw_output, None);
    }

    #[test]
    fn stripping_a_tool_call_frame_keeps_its_title_kind_status_and_id() {
        // Given — a completed Read tool-call frame
        let frame = frame_for_agent_activity(&a_completed_read_record());

        // When — the body is stripped
        let stripped = strip_tool_body(&frame);

        // Then — the lightweight metadata (and timestamp) is retained
        let (tc, at) = tool_call(&stripped);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-read-1");
        assert_eq!(tc.title, "Read main.rs L10-49");
        assert_eq!(tc.kind, ToolKind::Read as i32);
        assert_eq!(tc.status, ToolCallStatus::Completed as i32);
        assert_eq!(at, 3_000);
    }

    #[test]
    fn stripping_a_non_tool_frame_leaves_it_unchanged() {
        // Given — an agent-text frame (no tool body to strip)
        let frame = agent_text_frame("Analyzing the parser.", 1_000);

        // When — it is passed through the stripper
        let stripped = strip_tool_body(&frame);

        // Then — it is returned unchanged
        assert_eq!(stripped, frame);
    }

    // -----------------------------------------------------------------------
    // tool_call_detail — the on-demand body lookup GetAcpToolCallDetail is built on, resolved from
    // the same coalesced transcript view the stream replays.
    // -----------------------------------------------------------------------

    #[test]
    fn tool_call_detail_returns_the_full_bodies_for_a_recorded_call() {
        // Given — a session dir whose durable activity log holds one completed call
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();

        // When — its detail is looked up by tool_call_id
        let detail = tool_call_detail(dir.path(), "call-read-1")
            .expect("read transcript")
            .expect("call-read-1 is in the transcript");

        // Then — the full raw_input and raw_output are returned
        let raw_input: serde_json::Value =
            serde_json::from_str(&detail.raw_input.expect("raw_input")).expect("raw_input is JSON");
        let raw_output: serde_json::Value =
            serde_json::from_str(&detail.raw_output.expect("raw_output"))
                .expect("raw_output is JSON");
        assert_eq!(raw_input, a_read_input());
        assert_eq!(raw_output, serde_json::json!({ "content": "fn main() {}" }));
    }

    #[test]
    fn tool_call_detail_returns_none_for_an_unknown_tool_call_id() {
        // Given — a session dir holding one call
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_read_record()).unwrap();

        // When — an id not present in the transcript is looked up
        let detail = tool_call_detail(dir.path(), "no-such-call").expect("read transcript");

        // Then — no detail is found
        assert_eq!(detail, None);
    }

    // -----------------------------------------------------------------------
    // Paging: the tail page and the reverse cursor
    //
    // Both operate on the already-resolved transcript, so a `seq` means the same position to the
    // pager, the counter and the snapshot replay.
    // -----------------------------------------------------------------------

    /// A recorded transcript of `entry_count` agent-text frames, one second apart, labelled
    /// `Entry 1` … `Entry N` (**1-based**, so an assertion names the entry's position in the whole
    /// transcript rather than its index inside whichever page came back).
    fn a_recorded_transcript(entry_count: usize) -> Vec<AcpAgentMessage> {
        (1..=entry_count)
            .map(|n| agent_text_frame(&format!("Entry {n}"), 1_000 * n as i64))
            .collect()
    }

    /// The (oldest, newest) entry texts of a page — the boundaries, which is what paging decides.
    /// Panics on an empty page: a test asserting boundaries on nothing is asserting nothing.
    fn page_bounds(page: &TranscriptPage<'_>) -> (String, String) {
        let texts: Vec<String> = page.frames.iter().map(|f| agent_chunk(f).0).collect();
        let oldest = texts.first().expect("page carries no frames").clone();
        let newest = texts.last().expect("page carries no frames").clone();
        (oldest, newest)
    }

    #[test]
    fn tail_page_returns_the_newest_frames_stamped_with_the_seq_of_its_first_frame() {
        // Given — 250 recorded entries
        let transcript = a_recorded_transcript(250);

        // When — the newest page of 100 is taken
        let page = tail_page(&transcript, 100);

        // Then — it starts 150 frames in, and is not the head
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (150, 100, false)
        );
        // …carrying entries 151 → 250, oldest-first *within* the page
        assert_eq!(
            page_bounds(&page),
            ("Entry 151".to_string(), "Entry 250".to_string())
        );
    }

    #[test]
    fn a_transcript_shorter_than_the_page_size_tails_to_the_whole_transcript_at_its_head() {
        // Given — fewer entries than one page holds
        let transcript = a_recorded_transcript(40);

        // When
        let page = tail_page(&transcript, 100);

        // Then — the tail page IS the whole transcript, and it reaches the head
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (0, 40, true)
        );
        assert_eq!(
            page_bounds(&page),
            ("Entry 1".to_string(), "Entry 40".to_string())
        );
    }

    #[test]
    fn tail_page_of_an_empty_transcript_is_empty_and_at_the_head() {
        // Given — a session that recorded nothing
        let transcript: Vec<AcpAgentMessage> = Vec::new();

        // When
        let page = tail_page(&transcript, 100);

        // Then — an empty page at the head; there is nothing to page back to
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (0, 0, true)
        );
    }

    #[test]
    fn a_page_size_of_zero_falls_back_to_the_default_page_size() {
        // Given — twenty entries more than the server's default page holds
        let transcript = a_recorded_transcript(DEFAULT_REPLAY_PAGE_SIZE + 20);

        // When — the caller leaves `page_size` at the proto zero value
        let page = tail_page(&transcript, 0);

        // Then — the default bounds the page, rather than the whole transcript being served
        assert_eq!(
            (page.first_seq, page.frames.len()),
            (20, DEFAULT_REPLAY_PAGE_SIZE)
        );
    }

    #[test]
    fn page_before_returns_the_frames_immediately_older_than_the_cursor() {
        // Given — 250 entries, of which the newest page handed out a cursor of 150
        let transcript = a_recorded_transcript(250);

        // When — the reader scrolls back past it
        let page = page_before(&transcript, 150, 100);

        // Then — the 100 entries directly behind the cursor, with the head still further back
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (50, 100, false)
        );
        assert_eq!(
            page_bounds(&page),
            ("Entry 51".to_string(), "Entry 150".to_string())
        );
    }

    #[test]
    fn page_before_the_head_returns_nothing_and_reports_at_oldest() {
        // Given — a cursor already sitting at the transcript head
        let transcript = a_recorded_transcript(250);

        // When
        let page = page_before(&transcript, 0, 100);

        // Then — an empty page that says it is the head. `at_oldest` is why this is distinguishable
        // from a one-frame page at the head, which `first_seq == 0` alone could not express.
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (0, 0, true)
        );
    }

    #[test]
    fn page_before_reports_at_oldest_when_the_page_reaches_the_first_frame() {
        // Given — 150 entries: one tail page of 100 leaves exactly 50 behind it
        let transcript = a_recorded_transcript(150);

        // When — the reader pages back from that tail page's cursor
        let page = page_before(&transcript, 50, 100);

        // Then — a short page carrying the remaining entries, flagged as the head so the client
        // closes the range instead of fetching again
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (0, 50, true)
        );
        assert_eq!(
            page_bounds(&page),
            ("Entry 1".to_string(), "Entry 50".to_string())
        );
    }

    #[test]
    fn page_before_a_cursor_past_the_transcript_end_returns_the_newest_page() {
        // Given — a transcript rewritten shorter than the cursor a client is still holding
        let transcript = a_recorded_transcript(120);

        // When — the stale cursor points past its end
        let page = page_before(&transcript, 500, 100);

        // Then — the cursor clamps to the length, so the client gets a real page of the transcript
        // that exists now rather than nothing at all
        assert_eq!(
            (page.first_seq, page.frames.len(), page.at_oldest),
            (20, 100, false)
        );
        assert_eq!(
            page_bounds(&page),
            ("Entry 21".to_string(), "Entry 120".to_string())
        );
    }
}
