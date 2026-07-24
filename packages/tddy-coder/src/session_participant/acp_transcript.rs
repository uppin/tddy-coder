//! Persisting the coder's presenter events into the session's own ACP transcript
//! (`acp-transcript.jsonl`).
//!
//! The coder owns the ACP-mapped conversation for tool / cursor-cli sessions. As the presenter
//! broadcasts events, this seam maps each renderable one to an `AcpAgentMessage` frame and appends
//! it (stamped with the event's wall-clock time) so the read-only `StreamAcpReplay` can later
//! replay exactly what a live viewer would have seen. Non-renderable events are ignored.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;

use tddy_core::PresenterEvent;
use tddy_service::acp_replay::{agent_text_frame, append_acp_frame, frame_for_agent_activity};
use tddy_service::proto::acp::AcpAgentMessage;

/// Map a presenter event to the ACP transcript frame it produces (stamped at `at_unix_ms` for the
/// text case; the activity case is self-stamping from the record's own timestamps).
///
/// This is the single source of truth for the mapping shared by the on-disk writer
/// ([`append_frames_for_event`]) and the live `StreamAcpReplay` tail:
/// - [`PresenterEvent::AgentOutput`] → one `agent_message_chunk` frame carrying the text.
/// - [`PresenterEvent::AgentActivity`] → one enriched `tool_call` frame (title/kind/raw_input).
/// - every other variant → `None`; it carries nothing the transcript renders.
pub fn frame_for_event(event: &PresenterEvent, at_unix_ms: i64) -> Option<AcpAgentMessage> {
    match event {
        PresenterEvent::AgentOutput(text) => Some(agent_text_frame(text, at_unix_ms)),
        PresenterEvent::AgentActivity(record) => Some(frame_for_agent_activity(record)),
        _ => None,
    }
}

/// Append the ACP transcript frame that corresponds to `event`, stamped at `at_unix_ms`. Events
/// that produce no frame are a no-op (`Ok(())`).
pub fn append_frames_for_event(
    session_dir: &Path,
    event: &PresenterEvent,
    at_unix_ms: i64,
) -> std::io::Result<()> {
    match frame_for_event(event, at_unix_ms) {
        Some(frame) => append_acp_frame(session_dir, &frame),
        None => Ok(()),
    }
}

/// Current wall-clock time in Unix milliseconds (`0` before the epoch, which cannot occur).
pub(crate) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Spawn a task that persists the coder's presenter events into `session_dir`'s ACP transcript.
///
/// The task subscribes to the presenter broadcast and, for each event, appends the corresponding
/// frame(s) stamped at receive time. It tolerates a lagging receiver (skips the gap and continues)
/// and ends when the broadcast closes. Append failures are logged and skipped — a transcript write
/// must never take down the session.
pub fn spawn_acp_transcript_writer(
    mut rx: broadcast::Receiver<PresenterEvent>,
    session_dir: PathBuf,
) -> tokio::task::JoinHandle<()> {
    use tokio::sync::broadcast::error::RecvError;
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = append_frames_for_event(&session_dir, &event, now_unix_ms()) {
                        log::warn!(
                            target: "tddy_coder::session_participant::acp_transcript",
                            "append_frames_for_event: {}: {}",
                            session_dir.display(),
                            e
                        );
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use tddy_core::agent_activity::{AgentActivityRecord, STATUS_COMPLETED};
    use tddy_service::acp_replay::read_acp_transcript;
    use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};

    /// The (text, timestamp) of an `agent_message_chunk` frame (panics on any other shape).
    fn agent_chunk(frame: &tddy_service::proto::acp::AcpAgentMessage) -> (String, i64) {
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

    /// The (tool_call_id, title, raw_input, timestamp) of a `tool_call` frame.
    fn tool_call(
        frame: &tddy_service::proto::acp::AcpAgentMessage,
    ) -> (String, String, Option<String>, i64) {
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => (
                        tc.tool_call_id.map(|id| id.value).unwrap_or_default(),
                        tc.title,
                        tc.raw_input,
                        n.timestamp_unix_ms,
                    ),
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    fn a_completed_read_record() -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: "call-read-1".to_string(),
            tool_name: "Read".to_string(),
            input: serde_json::json!({ "file_path": "src/main.rs", "offset": 10, "limit": 40 }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "content": "fn main() {}" }),
            error_message: String::new(),
            started_unix_ms: 2_000,
            completed_unix_ms: 3_000,
            source: "coder".to_string(),
        }
    }

    #[test]
    fn an_agent_output_event_is_persisted_as_an_agent_text_frame() {
        // Given — an empty session dir
        let dir = tempfile::tempdir().unwrap();

        // When — an AgentOutput event is persisted
        append_frames_for_event(
            dir.path(),
            &PresenterEvent::AgentOutput("Analyzing the parser.".into()),
            1_000,
        )
        .unwrap();

        // Then — the transcript holds one agent-text frame carrying that text and timestamp
        let frames = read_acp_transcript(dir.path()).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            agent_chunk(&frames[0]),
            ("Analyzing the parser.".to_string(), 1_000)
        );
    }

    #[test]
    fn an_agent_activity_event_is_persisted_as_an_enriched_tool_frame() {
        // Given — an empty session dir
        let dir = tempfile::tempdir().unwrap();

        // When — a completed Read AgentActivity event is persisted
        append_frames_for_event(
            dir.path(),
            &PresenterEvent::AgentActivity(a_completed_read_record()),
            9_999,
        )
        .unwrap();

        // Then — the transcript holds one enriched tool_call frame; the frame is stamped from the
        // record (not the `at_unix_ms` argument), titled with the file/line range, and carries the
        // full input as raw_input.
        let frames = read_acp_transcript(dir.path()).unwrap();
        assert_eq!(frames.len(), 1);
        let (id, title, raw_input, at) = tool_call(&frames[0]);
        assert_eq!(id, "call-read-1");
        assert_eq!(title, "Read main.rs L10-49");
        assert_eq!(at, 3_000);
        let raw = raw_input.expect("raw_input should carry the full tool input");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("raw_input is JSON");
        assert_eq!(
            parsed,
            serde_json::json!({ "file_path": "src/main.rs", "offset": 10, "limit": 40 })
        );
    }
}
