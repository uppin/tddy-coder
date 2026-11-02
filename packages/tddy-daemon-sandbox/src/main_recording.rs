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

use tddy_task::{CaptureRecording, TerminalCapture};
use tokio::sync::Mutex;

/// A jailed session's `main` terminal registered for recording.
pub(crate) struct JailRecording {
    path: PathBuf,
    capture: Arc<StdMutex<TerminalCapture>>,
    /// Held for every write. `true` once nothing may write this recording again: the state was
    /// removed and its final snapshot written, or a later registration replaced this one.
    closed: Mutex<bool>,
}

/// The registrations, keyed by session id.
pub(crate) type JailRecordings = Mutex<HashMap<String, Arc<JailRecording>>>;

impl JailRecording {
    pub(crate) fn new(path: PathBuf, capture: Arc<StdMutex<TerminalCapture>>) -> Arc<Self> {
        Arc::new(Self {
            path,
            capture,
            closed: Mutex::new(false),
        })
    }

    /// Writes the final snapshot and closes the recording: no later periodic write follows it.
    pub(crate) async fn finish(&self) {
        let mut closed = self.closed.lock().await;
        if !*closed {
            self.write_snapshot().await;
            *closed = true;
        }
    }

    /// Closes the recording without a write; waits out a write in flight.
    pub(crate) async fn replace(&self) {
        *self.closed.lock().await = true;
    }

    /// Rewrites the recording every `interval` until it is closed.
    pub(crate) async fn record_periodically(self: Arc<Self>, interval: Duration) {
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
        let snapshot = CaptureRecording::of(&self.capture.lock().unwrap());
        let path = self.path.clone();
        let outcome = tokio::task::spawn_blocking(move || snapshot.write_atomically(&path)).await;
        let error = match outcome {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error.to_string(),
            Err(join_error) => join_error.to_string(),
        };
        log::warn!(
            target: "tddy_daemon_sandbox::main_recording",
            "could not write the jailed main terminal's recording to {}: {error}",
            self.path.display()
        );
    }
}
