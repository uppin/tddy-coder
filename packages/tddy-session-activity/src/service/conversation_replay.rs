//! The replay RPCs addressed at one subagent conversation of a session (`conversation_id`): the
//! conversation's own recorded transcript, served the way the session's is, then the frames the
//! conversation records while the stream is open. A conversation that recorded nothing is `NOT_FOUND`
//! naming it.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::path::Path;

use tddy_daemon_kernel::conversation_frames::{ConversationFrame, ConversationFrames};
use tddy_rpc::{Response, Status};
use tddy_service::acp_replay::{count_activity_entries, tail_page};
use tddy_service::proto::acp::AcpAgentMessage;
use tddy_service::proto::activity::{AcpReplayFrame, StreamMode};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_worktree_service::stream::MpscResultStream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::service::streamed;
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

/// A conversation-addressed `StreamAcpReplay`: the conversation's transcript as `mode` asks for it, then
/// every frame the conversation records after that.
///
/// The live channel is subscribed before the transcript is read. A frame recorded after the read is then
/// on the channel, and a frame recorded before it is in the transcript. The relay drops the live frames
/// the replay already covered, so a frame that falls in both is delivered once.
pub(crate) fn stream_conversation(
    frames: &ConversationFrames,
    session_dir: &Path,
    session_id: &str,
    conversation_id: &str,
    mode: StreamMode,
    page_size: u32,
) -> Result<Response<MpscResultStream<AcpReplayFrame>>, Status> {
    let live = frames.subscribe(session_id, conversation_id);
    let transcript = recorded_transcript(session_dir, session_id, conversation_id)?;
    if mode == StreamMode::CountThenLive {
        // A count carries no transcript payload, and it does not follow the conversation live.
        return Ok(streamed(vec![AcpReplayFrame {
            acp_agent_message: Vec::new(),
            activity_count: count_activity_entries(&transcript),
            // A count frame carries no transcript payload, so it has no position.
            seq: 0,
        }]));
    }
    let (first_seq, replayed) = replayed_window(&transcript, mode, page_size);
    // The first position the live relay has not covered: the replayed window ends just before it.
    let next_seq = first_seq + replayed.len() as u64;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    for (offset, frame) in replayed.iter().enumerate() {
        let frame = acp_replay_frame(frame, first_seq + offset as u64);
        if tx.send(Ok(frame)).is_err() {
            return Ok(Response::new(MpscResultStream::from(rx)));
        }
    }
    tokio::spawn(relay_conversation_tail(live, tx, next_seq));
    Ok(Response::new(MpscResultStream::from(rx)))
}

/// The slice of `transcript` one replay sends, and the position of its first frame in the transcript.
fn replayed_window(
    transcript: &[AcpAgentMessage],
    mode: StreamMode,
    page_size: u32,
) -> (u64, &[AcpAgentMessage]) {
    match mode {
        StreamMode::SnapshotThenLive => (0, transcript),
        StreamMode::TailThenLive => {
            let page = tail_page(transcript, usize::try_from(page_size).unwrap_or(usize::MAX));
            (page.first_seq, page.frames)
        }
        // Live-only replays none of the transcript and delivers what is recorded after subscribe.
        _ => (0, &[]),
    }
}

/// Relay the conversation's live frames from `next_seq` on, until the client goes away.
///
/// A frame below `next_seq` is one the replay already sent, so it is dropped. A reader that falls behind
/// the channel's capacity skips the frames it missed and keeps streaming from the oldest frame still held,
/// as the session's relays do. The seq of the first frame after a skip shows the gap to the client, and the
/// transcript on disk is what a reader reopens to read the missed frames.
async fn relay_conversation_tail(
    mut live: Receiver<ConversationFrame>,
    tx: UnboundedSender<Result<AcpReplayFrame, Status>>,
    next_seq: u64,
) {
    loop {
        match live.recv().await {
            Ok(published) if published.seq < next_seq => {}
            Ok(published) => {
                let frame = acp_replay_frame(&published.frame, published.seq);
                // A client that went away ends the relay at the next frame, as the session's relays do.
                if tx.send(Ok(frame)).is_err() {
                    break;
                }
            }
            // The channel dropped the oldest frames before this reader took them. The next `recv` is the
            // oldest frame still held, so the relay carries on from there.
            Err(RecvError::Lagged(missed)) => {
                log::debug!(
                    target: "tddy_session_activity::conversation_replay",
                    "a conversation replay fell {missed} frame(s) behind its recorder and skipped them"
                );
            }
            Err(RecvError::Closed) => break,
        }
    }
}
