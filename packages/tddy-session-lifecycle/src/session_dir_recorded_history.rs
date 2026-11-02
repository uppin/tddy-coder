//! The daemon's recordings of its sessions' agent terminals, read from the session directory.
//!
//! A session's `main` terminal is recorded at `<tddy_data_dir>/sessions/<id>/terminals/main.capture`
//! (see [`MAIN_CAPTURE_RECORDING`]). That file is what a stopped session's history is served from,
//! by this daemon or by a later one over the same data dir. No other terminal is recorded.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_task::CaptureRecording;
use tddy_terminal_rpc::RecordedTerminalHistory;

/// Where, relative to a session directory, its `main` terminal is recorded.
pub const MAIN_CAPTURE_RECORDING: &str = "terminals/main.capture";

/// The recording of `session_id`'s `main` terminal under `tddy_data_dir`.
#[must_use]
pub fn main_capture_recording_path(tddy_data_dir: &Path, session_id: &str) -> PathBuf {
    tddy_core::session_lifecycle::unified_session_dir_path(tddy_data_dir, session_id)
        .join(MAIN_CAPTURE_RECORDING)
}

/// [`RecordedTerminalHistory`] over this daemon's session directories.
pub struct SessionDirRecordedHistory {
    tddy_data_dir: PathBuf,
}

impl SessionDirRecordedHistory {
    #[must_use]
    pub fn new(tddy_data_dir: PathBuf) -> Self {
        SessionDirRecordedHistory { tddy_data_dir }
    }
}

#[async_trait]
impl RecordedTerminalHistory for SessionDirRecordedHistory {
    async fn recorded(
        &self,
        os_user: &str,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<CaptureRecording> {
        // TODO(agent-history): implement
        let _ = (&self.tddy_data_dir, os_user, session_id, terminal_id);
        None
    }
}
