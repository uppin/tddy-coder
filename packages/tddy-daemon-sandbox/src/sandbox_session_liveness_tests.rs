//! [`SandboxSessionManager::live_sessions`]: a sandboxed session is live while its runner runs.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tddy_task::TerminalCapture;
use tddy_testing_commons::wait::eventually_awaiting;
use tokio::sync::{broadcast, mpsc};

use crate::sandbox_session::{SandboxSessionManager, SandboxSessionState, SandboxSessionStateInit};

/// A safety net for a runner's exit to be observed.
const OBSERVE: Duration = Duration::from_secs(10);

/// A session state over a plain process standing in for `tddy-sandbox-runner`.
fn a_sandbox_running(
    program: &str,
    args: &[&str],
) -> (Arc<SandboxSessionState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in runner starts");
    let pid = child.id();
    let handle = tddy_sandbox::SandboxHandle::new(
        child,
        tmp.path().join("profile.sb"),
        tmp.path().join("unused.grpc.sock"),
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
async fn a_sandboxed_session_whose_runner_runs_is_live() {
    // Given
    let manager = SandboxSessionManager::new();
    let (state, _tmp) = a_sandbox_running("sleep", &["300"]);
    manager.insert("jailed-running".to_string(), state).await;

    // When
    let live = manager.live_sessions().await;

    // Then
    assert!(live.contains("jailed-running"));
}

#[tokio::test]
async fn a_sandboxed_session_whose_runner_exited_is_not_live() {
    // Given a registered session whose runner exits by itself
    let manager = SandboxSessionManager::new();
    let (state, _tmp) = a_sandbox_running("true", &[]);
    manager.insert("jailed-exited".to_string(), state).await;

    // When / Then it is not live once the runner has exited, though its entry remains
    eventually_awaiting("the exited runner to stop being live", OBSERVE, || async {
        (!manager.live_sessions().await.contains("jailed-exited"))
            .then_some(())
            .ok_or_else(|| "still live".to_string())
    })
    .await;
    assert!(manager.get("jailed-exited").await.is_some());
}
