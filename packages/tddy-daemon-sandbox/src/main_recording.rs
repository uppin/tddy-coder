//! Recording a jailed session's `main` terminal to disk, so its history outlives the jail.
//!
//! The jailed counterpart of the CLI session manager's recorder. A session registers where its
//! `main` capture is recorded; the retained window is rewritten there every
//! [`tddy_task::RECORDING_INTERVAL`] while the jail runs, and once more when the session's sandbox
//! state is removed — at delete, or when a resume replaces it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tddy_task::{CaptureRecording, TerminalCapture, RECORDING_INTERVAL};
use tokio::sync::Mutex;

use crate::sandbox_session::SandboxSessionManager;

/// The jailed sessions whose `main` terminal is recorded, keyed by session id, and how often a
/// running one is rewritten.
pub(crate) struct JailRecorder {
    recordings: Mutex<HashMap<String, Arc<JailRecording>>>,
    interval: Duration,
}

impl Default for JailRecorder {
    fn default() -> Self {
        Self {
            recordings: Mutex::new(HashMap::new()),
            interval: RECORDING_INTERVAL,
        }
    }
}

impl JailRecorder {
    /// Writes the final snapshot of `session_id`'s recording, when it is recorded, and stops
    /// recording it.
    pub(crate) async fn finish(&self, session_id: &str) {
        let recording = self.recordings.lock().await.remove(session_id);
        if let Some(recording) = recording {
            recording.finish().await;
        }
    }
}

impl SandboxSessionManager {
    /// The same manager, rewriting running recordings every `interval` instead of
    /// [`RECORDING_INTERVAL`].
    #[must_use]
    pub fn with_recording_interval(mut self, interval: Duration) -> Self {
        self.recorder.interval = interval;
        self
    }

    /// Record `session_id`'s jailed `main` terminal at `path`: periodically while it runs, and once
    /// more when its sandbox state is removed — the jailed counterpart of
    /// `CliSessionManager::record_main_capture`.
    ///
    /// Registering again replaces the earlier registration, which stops writing. A session with no
    /// sandbox state has nothing to record, so nothing is registered and that is logged.
    pub async fn record_main_capture(&self, session_id: &str, path: PathBuf) {
        let Some(state) = self.get(session_id).await else {
            log::warn!(
                target: "tddy_daemon_sandbox::main_recording",
                "record_main_capture: session={session_id} has no sandbox state; {} is not registered",
                path.display()
            );
            return;
        };
        let recording = JailRecording::new(path, Arc::clone(&state.capture));
        let replaced = self
            .recorder
            .recordings
            .lock()
            .await
            .insert(session_id.to_string(), Arc::clone(&recording));
        if let Some(replaced) = replaced {
            replaced.replace().await;
        }
        tokio::spawn(recording.record_periodically(self.recorder.interval));
    }
}

/// A jailed session's `main` terminal registered for recording.
struct JailRecording {
    path: PathBuf,
    capture: Arc<StdMutex<TerminalCapture>>,
    /// Held for every write. `true` once nothing may write this recording again: the state was
    /// removed and its final snapshot written, or a later registration replaced this one.
    closed: Mutex<bool>,
}

impl JailRecording {
    fn new(path: PathBuf, capture: Arc<StdMutex<TerminalCapture>>) -> Arc<Self> {
        Arc::new(Self {
            path,
            capture,
            closed: Mutex::new(false),
        })
    }

    /// Writes the final snapshot and closes the recording: no later periodic write follows it.
    async fn finish(&self) {
        let mut closed = self.closed.lock().await;
        if !*closed {
            self.write_snapshot().await;
            *closed = true;
        }
    }

    /// Closes the recording without a write; waits out a write in flight.
    async fn replace(&self) {
        *self.closed.lock().await = true;
    }

    /// Rewrites the recording every `interval` until it is closed.
    async fn record_periodically(self: Arc<Self>, interval: Duration) {
        loop {
            tokio::time::sleep(interval).await;
            let closed = self.closed.lock().await;
            if *closed {
                return;
            }
            self.write_snapshot().await;
        }
    }

    /// A failed write is logged and the session carries on: the recording is a copy.
    async fn write_snapshot(&self) {
        if let Err(error) = CaptureRecording::record(&self.capture, &self.path).await {
            log::warn!(
                target: "tddy_daemon_sandbox::main_recording",
                "could not write the jailed main terminal's recording to {}: {error}",
                self.path.display()
            );
        }
    }
}
