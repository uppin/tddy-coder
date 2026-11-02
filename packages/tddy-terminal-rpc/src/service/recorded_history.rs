//! Where the history of a terminal that is no longer running comes from.
//!
//! A running terminal answers `GetTerminalHistory` from its live capture. Once its process has
//! exited the capture is gone with it, so a host that records terminals answers from the recording
//! instead — and only then: a running terminal never reads a recording.

use async_trait::async_trait;
use tddy_task::CaptureRecording;

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
