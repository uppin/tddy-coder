//! Recording a session's agent terminal to disk, so its history outlives the process.
//!
//! A session registers where its `main` terminal is recorded. While `main` runs, its capture's
//! retained window is rewritten there every [`RECORDING_INTERVAL`] — a daemon that crashes loses at
//! most that much — and once more when the process exits, before the terminal leaves the index, so
//! the recording always ends with the last thing the agent printed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_task::{CaptureRecording, TaskId, TerminalCapture};
use tokio::sync::{Mutex, RwLock};

use super::{CliSessionManager, TerminalIndex, MAIN_TERMINAL_ID};

pub use tddy_task::RECORDING_INTERVAL;

/// Where, relative to a session directory, its `main` terminal is recorded.
pub const MAIN_CAPTURE_RECORDING: &str = "terminals/main.capture";

/// The recording of `session_id`'s `main` terminal under `tddy_data_dir`:
/// `<tddy_data_dir>/sessions/<id>/terminals/main.capture`.
#[must_use]
pub fn main_capture_recording_path(tddy_data_dir: &Path, session_id: &str) -> PathBuf {
    tddy_core::session_lifecycle::unified_session_dir_path(tddy_data_dir, session_id)
        .join(MAIN_CAPTURE_RECORDING)
}

/// The sessions whose `main` terminal is recorded: `session_id → recording`.
pub(crate) type MainRecordings = Arc<RwLock<HashMap<String, Arc<MainRecording>>>>;

/// A session's `main` terminal registered for recording.
pub(crate) struct MainRecording {
    /// The task of the `main` terminal this recording follows. A later run of the session has another.
    task_id: TaskId,
    /// Where the recording is written.
    path: PathBuf,
    /// The terminal's capture, which its PTY appends to.
    capture: Arc<std::sync::Mutex<TerminalCapture>>,
    /// Held for every write. `true` once nothing may write this recording again: its run has
    /// exited, or a later registration replaced it. Holding it is what keeps a periodic write from
    /// landing after the exit write with an older snapshot.
    closed: Mutex<bool>,
}

impl MainRecording {
    /// Snapshot the capture and write it to the recording's path. A failed write is logged and the
    /// terminal carries on: the recording is a copy, and the live terminal does not depend on it.
    async fn write_snapshot(&self) {
        if let Err(error) = CaptureRecording::record(&self.capture, &self.path).await {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not write the main terminal's recording to {}: {error}",
                self.path.display()
            );
        }
    }
}

impl CliSessionManager {
    /// The same manager, rewriting running recordings every `interval` instead of
    /// [`RECORDING_INTERVAL`].
    #[must_use]
    pub fn with_recording_interval(mut self, interval: Duration) -> Self {
        self.recording_interval = interval;
        self
    }

    /// Record `session_id`'s running `main` terminal at `path`: periodically while it runs, and once
    /// more at its exit, before it leaves the index.
    ///
    /// Registering again replaces the earlier registration, which stops writing. A session with no
    /// running `main` has nothing to record, so nothing is registered and that is logged.
    pub async fn record_main_capture(&self, session_id: &str, path: PathBuf) {
        let Some(main) = self.resolve_pty_handle(session_id, MAIN_TERMINAL_ID).await else {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "record_main_capture: session={session_id} has no running main terminal; {} is not registered",
                path.display()
            );
            return;
        };
        let recording = Arc::new(MainRecording {
            task_id: main.task_id.clone(),
            path,
            capture: Arc::clone(&main.capture),
            closed: Mutex::new(false),
        });
        let replaced = self
            .main_recordings
            .write()
            .await
            .insert(session_id.to_string(), Arc::clone(&recording));
        if let Some(replaced) = replaced {
            // Waits out a write the earlier registration is making, so none of its writes lands after this call.
            *replaced.closed.lock().await = true;
        }
        tokio::spawn(record_periodically(
            recording,
            Arc::clone(&self.terminals),
            session_id.to_string(),
            self.recording_interval,
        ));
    }
}

/// Rewrites `recording` every `interval`, for as long as it is open and its `main` terminal is still
/// in the index.
async fn record_periodically(
    recording: Arc<MainRecording>,
    terminals: TerminalIndex,
    session_id: String,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        let closed = recording.closed.lock().await;
        if *closed || !is_indexed(&terminals, &session_id, &recording.task_id).await {
            return;
        }
        recording.write_snapshot().await;
    }
}

/// Writes the exit snapshot of `session_id`'s recording when its `main` run `task_id` has exited, and
/// closes it. Called before the terminal leaves the index.
pub(crate) async fn finish_main_recording(
    recordings: &MainRecordings,
    session_id: &str,
    task_id: &TaskId,
) {
    let Some(recording) = take_registration(recordings, session_id, task_id).await else {
        return;
    };
    let mut closed = recording.closed.lock().await;
    recording.write_snapshot().await;
    *closed = true;
}

/// Removes and returns the registration of `session_id` if it records the run `task_id`.
async fn take_registration(
    recordings: &MainRecordings,
    session_id: &str,
    task_id: &TaskId,
) -> Option<Arc<MainRecording>> {
    let mut registered = recordings.write().await;
    if registered
        .get(session_id)
        .is_some_and(|recording| recording.task_id == *task_id)
    {
        registered.remove(session_id)
    } else {
        None
    }
}

/// Whether `session_id`'s `main` terminal is still in the index as the run `task_id`.
async fn is_indexed(terminals: &TerminalIndex, session_id: &str, task_id: &TaskId) -> bool {
    terminals
        .read()
        .await
        .get(session_id)
        .and_then(|tools| tools.get(MAIN_TERMINAL_ID))
        .is_some_and(|entry| entry.task_id == *task_id)
}
