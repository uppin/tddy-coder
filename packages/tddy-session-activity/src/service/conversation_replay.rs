//! The replay RPCs addressed at one subagent conversation of a session (`conversation_id`): the
//! conversation's own recorded transcript, served the way the session's is, or `NOT_FOUND` naming the
//! conversation when it recorded nothing.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::path::Path;

use tddy_rpc::Status;
use tddy_service::acp_replay::{count_activity_entries, tail_page};
use tddy_service::proto::acp::AcpAgentMessage;
use tddy_service::proto::activity::{AcpReplayFrame, StreamMode};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;

use crate::streams::acp_replay_frame;

/// `conversation_id`'s recorded transcript. A conversation that recorded nothing — an in-jail loop, or
/// an id this session never ran — is `NOT_FOUND`, naming it.
pub(crate) fn recorded_transcript(
    session_dir: &Path,
    session_id: &str,
    conversation_id: &str,
) -> Result<Vec<AcpAgentMessage>, Status> {
    read_conversation_transcript(session_dir, conversation_id)
        .map_err(|e| Status::internal(format!("read transcript: {e}")))?
        .ok_or_else(|| {
            Status::not_found(format!(
                "conversation '{conversation_id}' recorded nothing in session '{session_id}'"
            ))
        })
}

/// The frames one replay of `transcript` sends, as `mode` asks: the running entry count for a
/// count-first stream, otherwise the transcript — every frame, its newest `page_size` frames, or none —
/// each stamped with its absolute position in the transcript.
///
/// TODO(conversation-bubble): a conversation's live frames are not published to the hub, so this replay
/// ends with the recorded transcript and the dialog does not follow a turn that is still running.
pub(crate) fn replay_frames(
    transcript: &[AcpAgentMessage],
    mode: StreamMode,
    page_size: u32,
) -> Vec<AcpReplayFrame> {
    if mode == StreamMode::CountThenLive {
        return vec![AcpReplayFrame {
            acp_agent_message: Vec::new(),
            activity_count: count_activity_entries(transcript),
            // A count frame carries no transcript payload, so it has no position.
            seq: 0,
        }];
    }
    let (first_seq, replayed) = match mode {
        StreamMode::SnapshotThenLive => (0, transcript),
        StreamMode::TailThenLive => {
            let page = tail_page(transcript, usize::try_from(page_size).unwrap_or(usize::MAX));
            (page.first_seq, page.frames)
        }
        _ => (0, &[][..]),
    };
    replayed
        .iter()
        .enumerate()
        .map(|(offset, frame)| acp_replay_frame(frame, first_seq + offset as u64))
        .collect()
}
