//! `ReadSessionActivity` and `ReadSessionTranscript` read on this host (`#start-parity` 6/9).
//!
//! What the tools page ([`tddy_tool_engine::session_follow`]) is read here from the target
//! session's directory: its whole activity log and pending question, and the transcript frames the
//! web's Activities view pages (`GetAcpReplayPage`) with their tool bodies stripped, through the
//! same `tddy_service::acp_replay` readers that RPC uses. A call naming another host is forwarded to
//! that host whole and read there.

use std::path::{Path, PathBuf};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_service::proto::acp::{
    acp_agent_message, content_block, session_update, AcpAgentMessage, ContentChunk,
};
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_worktree_service::session_target::SessionTarget;

fn session_dir(sessions_base: &Path, target: &SessionTarget) -> PathBuf {
    unified_session_dir_path(sessions_base, &target.session_id)
}

/// Every call `target` has made, uncapped, and the question it waits on.
pub(super) async fn session_activity(
    sessions_base: &Path,
    target: &SessionTarget,
) -> anyhow::Result<SessionActivity> {
    let session_dir = session_dir(sessions_base, target);
    tokio::task::spawn_blocking(move || {
        Ok(SessionActivity {
            records: tddy_core::agent_activity::read_all_agent_activity(&session_dir)?,
            pending_question: tddy_core::pending_question::read_pending_question(&session_dir)?,
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!("reading the session's activity failed: {e}"))?
}

/// The frames `GetAcpReplayPage` pages, bodies stripped the way it strips them, one item each.
pub(super) async fn session_transcript(
    sessions_base: &Path,
    target: &SessionTarget,
) -> anyhow::Result<Vec<TranscriptItem>> {
    let session_dir = session_dir(sessions_base, target);
    tokio::task::spawn_blocking(move || {
        let frames = tddy_service::acp_replay::read_session_transcript(&session_dir)?;
        Ok(frames
            .iter()
            .enumerate()
            .map(|(seq, frame)| transcript_item(seq as u64, frame))
            .collect())
    })
    .await
    .map_err(|e| anyhow::anyhow!("reading the session's transcript failed: {e}"))?
}

/// One tool call's whole input and output (`GetAcpToolCallDetail`'s answer), or `None` for an id
/// the session never made.
pub(super) async fn session_tool_call(
    sessions_base: &Path,
    target: &SessionTarget,
    tool_call_id: &str,
) -> anyhow::Result<Option<ToolCallBodies>> {
    let session_dir = session_dir(sessions_base, target);
    let tool_call_id = tool_call_id.to_string();
    tokio::task::spawn_blocking(move || {
        Ok(
            tddy_service::acp_replay::tool_call_detail(&session_dir, &tool_call_id)?.map(
                |detail| ToolCallBodies {
                    raw_input: detail.raw_input,
                    raw_output: detail.raw_output,
                },
            ),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("reading the tool call failed: {e}"))?
}

/// One ACP frame as a transcript item: its kind, and the text it carries (a tool call's title).
fn transcript_item(seq: u64, frame: &AcpAgentMessage) -> TranscriptItem {
    let update = match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            n.update.as_ref().and_then(|u| u.update.as_ref())
        }
        _ => None,
    };
    let (kind, text, tool_call_id) = match update {
        Some(session_update::Update::UserMessageChunk(c)) => ("user_message", chunk_text(c), ""),
        Some(session_update::Update::AgentMessageChunk(c)) => ("agent_message", chunk_text(c), ""),
        Some(session_update::Update::AgentThoughtChunk(c)) => ("agent_thought", chunk_text(c), ""),
        Some(session_update::Update::ToolCall(tc)) => (
            "tool_call",
            tc.title.clone(),
            tc.tool_call_id.as_ref().map_or("", |id| id.value.as_str()),
        ),
        Some(session_update::Update::ToolCallUpdate(tc)) => (
            "tool_call",
            tc.fields
                .as_ref()
                .and_then(|f| f.title.clone())
                .unwrap_or_default(),
            tc.tool_call_id.as_ref().map_or("", |id| id.value.as_str()),
        ),
        Some(session_update::Update::Plan(_)) => ("plan", String::new(), ""),
        _ => ("other", String::new(), ""),
    };
    TranscriptItem {
        seq,
        kind: kind.to_string(),
        text,
        tool_call_id: tool_call_id.to_string(),
        truncated: false,
    }
}

fn chunk_text(chunk: &ContentChunk) -> String {
    match chunk.content.as_ref().and_then(|c| c.block.as_ref()) {
        Some(content_block::Block::Text(t)) => t.text.clone(),
        _ => String::new(),
    }
}
