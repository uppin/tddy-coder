//! A jailed session's `main` terminal is recorded to disk while the jail runs, and once more when
//! the session's sandbox state is removed. The state is driven directly: no jail is started.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_daemon_sandbox::sandbox_session::{
    SandboxSessionManager, SandboxSessionState, SandboxSessionStateInit,
};
use tddy_task::{CaptureRecording, TerminalCapture};
use tokio::sync::{broadcast, mpsc};

const SESSION: &str = "jailed-session";
const INTERVAL: Duration = Duration::from_millis(50);

fn a_registered_jail(manager: &SandboxSessionManager) -> Arc<Mutex<TerminalCapture>> {
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
    futures_block(manager.insert(SESSION.to_string(), Arc::new(state)));
    capture
}

fn futures_block<F: std::future::Future>(future: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

async fn recording_holding(path: &Path, text: &str) -> CaptureRecording {
    for _ in 0..200 {
        if let Ok(recording) = CaptureRecording::read(path) {
            if recording.data.ends_with(text.as_bytes()) {
                return recording;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "the recording at {} never ended with {text:?}",
        path.display()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_jailed_main_is_recorded_on_the_interval() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals/main.capture");
    let manager = SandboxSessionManager::new().with_recording_interval(INTERVAL);
    let capture = a_registered_jail(&manager);
    capture.lock().unwrap().append(b"first");

    manager.record_main_capture(SESSION, path.clone()).await;

    recording_holding(&path, "first").await;
    capture.lock().unwrap().append(b" second");
    recording_holding(&path, "first second").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_the_state_writes_the_last_output_before_it_returns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminals/main.capture");
    // An interval no test waits out: only the removal can have written the final output.
    let manager = SandboxSessionManager::new().with_recording_interval(Duration::from_secs(3600));
    let capture = a_registered_jail(&manager);
    manager.record_main_capture(SESSION, path.clone()).await;
    capture.lock().unwrap().append(b"the last words");

    let removed = manager.remove(SESSION).await;

    assert!(removed.is_some());
    let recording = CaptureRecording::read(&path).expect("removal wrote the recording");
    assert_eq!(recording.data, b"the last words");
    assert_eq!(recording.end_offset, 14);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_without_sandbox_state_is_never_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.capture");
    let manager = SandboxSessionManager::new().with_recording_interval(INTERVAL);

    manager.record_main_capture("nobody", path.clone()).await;
    tokio::time::sleep(INTERVAL * 4).await;

    assert!(!path.exists());
}
