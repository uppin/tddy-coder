//! `SandboxSessionManager::stop_all`: a daemon shutting down stops every session's **main** jail,
//! which it used to leave running
//! (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`, part 1).

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use pretty_assertions::assert_eq;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

use crate::sandbox_session::{SandboxSessionManager, SandboxSessionState, SandboxSessionStateInit};

/// A registered main jail whose runner is a `sleep`.
fn a_main_jail_running() -> (Arc<SandboxSessionState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let child = Command::new("sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in runner starts");
    let pid = child.id();
    let handle = tddy_sandbox::SandboxHandle::new(
        child,
        tmp.path().join("profile.sb"),
        tmp.path().join("unused.sock"),
        tmp.path().join("sandbox.ready"),
    );
    let (stdout_tx, _) = broadcast::channel(4);
    let (stdin_tx, _) = mpsc::unbounded_channel();
    let state = SandboxSessionState::new(SandboxSessionStateInit {
        pid,
        worktree_path: tmp.path().to_path_buf(),
        stdout_tx,
        capture: Arc::new(StdMutex::new(TerminalCapture::new())),
        stdin_tx,
        ready_marker: tmp.path().join("sandbox.ready"),
        handle,
        managed_workflow: None,
    });
    (Arc::new(state), tmp)
}

#[tokio::test]
async fn stop_all_stops_every_main_jail_and_forgets_it() {
    // Given two sessions' main jails
    let manager = SandboxSessionManager::new();
    let (first, _first_dir) = a_main_jail_running();
    let (second, _second_dir) = a_main_jail_running();
    manager
        .insert("session-b".to_string(), Arc::clone(&second))
        .await;
    manager
        .insert("session-a".to_string(), Arc::clone(&first))
        .await;

    // When
    let stopped = manager.stop_all().await;

    // Then both runners are stopped, named in order, and neither session is registered any more
    assert_eq!(
        stopped,
        vec!["session-a".to_string(), "session-b".to_string()]
    );
    assert_eq!((first.is_running(), second.is_running()), (false, false));
    assert!(manager.get("session-a").await.is_none());
    assert!(manager.get("session-b").await.is_none());
}

#[tokio::test]
async fn stop_all_with_no_main_jail_stops_nothing() {
    // Given
    let manager = SandboxSessionManager::new();

    // When
    let stopped = manager.stop_all().await;

    // Then
    assert_eq!(stopped, Vec::<String>::new());
}
