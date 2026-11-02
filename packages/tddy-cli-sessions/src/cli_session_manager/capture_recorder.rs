//! Recording a session's agent terminal to disk, so its history outlives the process.
//!
//! A session registers where its `main` terminal is recorded. While `main` runs, its capture's
//! retained window is rewritten there every [`RECORDING_INTERVAL`] — a daemon that crashes loses at
//! most that much — and once more when the process exits, before the terminal leaves the index, so
//! the recording always ends with the last thing the agent printed.

use std::path::PathBuf;
use std::time::Duration;

use super::CliSessionManager;

/// How often a running `main` terminal's capture is rewritten to its recording.
pub const RECORDING_INTERVAL: Duration = Duration::from_secs(30);

impl CliSessionManager {
    /// The same manager, rewriting running recordings every `interval` instead of
    /// [`RECORDING_INTERVAL`].
    #[must_use]
    pub fn with_recording_interval(self, interval: Duration) -> Self {
        // TODO(agent-history): implement
        let _ = interval;
        self
    }

    /// Record `session_id`'s `main` terminal at `path`: periodically while it runs, and at exit.
    pub async fn record_main_capture(&self, session_id: &str, path: PathBuf) {
        // TODO(agent-history): implement
        let _ = (session_id, path);
    }
}
