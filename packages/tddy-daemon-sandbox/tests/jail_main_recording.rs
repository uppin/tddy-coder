//! A jailed session's `main` terminal is recorded to disk while the jail runs, and once more when
//! the session's sandbox state is removed. The state is driven directly: no jail is started.
//!
//! Feature: docs/ft/daemon/terminal-sessions.md § History of a stopped agent terminal

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_daemon_sandbox::sandbox_session::{
    SandboxSessionManager, SandboxSessionState, SandboxSessionStateInit,
};
use tddy_task::{CaptureRecording, TerminalCapture};
use tddy_testing_commons::wait::eventually;
use tokio::sync::{broadcast, mpsc};

const SESSION: &str = "jailed-session";
const INTERVAL: Duration = Duration::from_millis(50);

/// A safety net for the periodic recorder to write, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(5);

/// Registers sandbox state for [`SESSION`] and returns the capture its jail would append to.
async fn a_registered_jail(manager: &SandboxSessionManager) -> Arc<Mutex<TerminalCapture>> {
    let capture = Arc::new(Mutex::new(TerminalCapture::new()));
    let (stdout_tx, _) = broadcast::channel(8);
    let (stdin_tx, _stdin_rx) = mpsc::unbounded_channel();
    let child = std::process::Command::new("true")
        .spawn()
        .expect("a trivial child process");
    let unused = PathBuf::from("/unused-by-the-recording");
    let state = SandboxSessionState::new(SandboxSessionStateInit {
        pid: 4242,
        worktree_path: unused.clone(),
        stdout_tx,
        capture: Arc::clone(&capture),
        stdin_tx,
        ready_marker: unused.clone(),
        handle: tddy_sandbox::SandboxHandle::new(child, unused.clone(), unused.clone(), unused),
        managed_workflow: None,
    });
    manager.insert(SESSION.to_string(), Arc::new(state)).await;
    capture
}

/// The recording at `path`, once it ends with `text`.
async fn recording_ending_with(path: &Path, text: &str) -> CaptureRecording {
    eventually(
        "the recording to end with the expected output",
        OBSERVE,
        || {
            let recording = CaptureRecording::read(path)
                .map_err(|e| format!("{} is not a readable recording yet: {e}", path.display()))?;
            if recording.data.ends_with(text.as_bytes()) {
                Ok(recording)
            } else {
                Err(format!(
                    "the recording holds {:?}, not yet ending with {text:?}",
                    String::from_utf8_lossy(&recording.data)
                ))
            }
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_jailed_main_is_recorded_on_the_interval() {
    // Given a jail that has printed, recorded every 50ms
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals/main.capture");
    let manager = SandboxSessionManager::new().with_recording_interval(INTERVAL);
    let capture = a_registered_jail(&manager).await;
    capture.lock().unwrap().append(b"first");

    // When its main terminal is registered for recording
    manager.record_main_capture(SESSION, path.clone()).await;

    // Then what it printed reaches the disk, and so does what it prints next
    recording_ending_with(&path, "first").await;
    capture.lock().unwrap().append(b" second");
    recording_ending_with(&path, "first second").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_the_state_writes_the_last_output_before_it_returns() {
    // Given a recorded jail on an interval no test waits out: only the removal can write
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals/main.capture");
    let manager = SandboxSessionManager::new().with_recording_interval(Duration::from_secs(3600));
    let capture = a_registered_jail(&manager).await;
    manager.record_main_capture(SESSION, path.clone()).await;
    capture.lock().unwrap().append(b"the last words");

    // When its sandbox state is removed
    let removed = manager.remove(SESSION).await;

    // Then the recording already holds the last output, at its offsets
    assert!(removed.is_some());
    let recording = CaptureRecording::read(&path).expect("removal wrote the recording");
    assert_eq!(recording.data, b"the last words");
    assert_eq!(recording.end_offset, 14);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_without_sandbox_state_is_never_recorded() {
    // Given a manager holding no state for the session
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.capture");
    let manager = SandboxSessionManager::new().with_recording_interval(INTERVAL);

    // When a recording is registered for it and several intervals pass
    manager.record_main_capture("nobody", path.clone()).await;
    tokio::time::sleep(INTERVAL * 4).await;

    // Then nothing is written
    assert!(!path.exists());
}
