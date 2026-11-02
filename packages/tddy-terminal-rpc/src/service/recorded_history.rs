//! Where the history of a terminal that is no longer running comes from.
//!
//! A running terminal answers `GetTerminalHistory` from its live capture. Once its process has
//! exited the capture is gone with it, so a host that records terminals answers from the recording
//! instead — and only then: a running terminal never reads a recording.

use async_trait::async_trait;
use tddy_rpc::Status;
use tddy_task::CaptureRecording;
use tokio::sync::mpsc;

use crate::bridge::resolved_terminal_id;
use crate::proto::terminal_session::{GetTerminalHistoryRequest, TerminalHistoryChunk};

/// The recorded history of a terminal that is not running, as its host kept it.
#[async_trait]
pub trait RecordedTerminalHistory: Send + Sync {
    /// The recording of `terminal_id` in `session_id`, read on behalf of `os_user`, or `None` when
    /// this host recorded nothing for it.
    async fn recorded(
        &self,
        os_user: &str,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<CaptureRecording>;
}

/// A host that records no terminal: a stopped terminal has no history.
pub struct NoRecordedHistory;

#[async_trait]
impl RecordedTerminalHistory for NoRecordedHistory {
    async fn recorded(&self, _: &str, _: &str, _: &str) -> Option<CaptureRecording> {
        None
    }
}

/// Serve `GetTerminalHistory` for a terminal that is not running: one forward chunk of its recording,
/// paged as the live capture pages, then the stream closes.
///
/// The request's limits are read as the live path reads them. `default_max_bytes` is the per-frame
/// budget a request that leaves `max_bytes` at zero is given.
pub(super) async fn serve_recorded_history(
    recorded: &dyn RecordedTerminalHistory,
    os_user: &str,
    req: &GetTerminalHistoryRequest,
    default_max_bytes: usize,
) -> Result<mpsc::Receiver<Result<TerminalHistoryChunk, Status>>, Status> {
    let terminal_id = resolved_terminal_id(&req.terminal_id);
    let recording = recorded
        .recorded(os_user, &req.session_id, terminal_id)
        .await
        .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

    let max_bytes = if req.max_bytes == 0 {
        default_max_bytes
    } else {
        req.max_bytes as usize
    };
    let chunk = recording.replay_from(req.from_offset, req.until_offset, max_bytes);

    let (tx, rx) = mpsc::channel(1);
    let _ = tx
        .send(Ok(TerminalHistoryChunk {
            data: chunk.data,
            start_offset: chunk.start_offset,
            end_offset: chunk.end_offset,
            at_oldest: chunk.at_oldest,
            at_end: chunk.at_end,
        }))
        .await;
    Ok(rx)
}
