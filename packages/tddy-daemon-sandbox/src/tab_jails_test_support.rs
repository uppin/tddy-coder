//! What the `TabJails` suites share: the launch of a tab's jail, and a jail whose "runner" is a
//! `sleep` — what the registry owns is the runner's lifetime, which a real process shows and a kernel
//! sandbox adds nothing to.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

use super::{TabJailLaunch, TabJailPaths};
use crate::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};

/// The launch of tab `terminal_id`'s jail, with paths that only have to be distinct.
pub(super) fn a_launch(session_id: &str, terminal_id: &str) -> TabJailLaunch {
    let root = PathBuf::from("/sessions")
        .join(session_id)
        .join("sandbox/tabs")
        .join(terminal_id);
    TabJailLaunch {
        session_id: session_id.to_string(),
        terminal_id: terminal_id.to_string(),
        session_dir: PathBuf::from("/sessions").join(session_id),
        worktree: PathBuf::from("/worktrees").join(session_id),
        paths: TabJailPaths {
            root: root.clone(),
            ipc_socket: root.join("tab.sock"),
            ready_marker: root.join("sandbox.ready"),
            exec_plan: root.join("exec-plan.json"),
            profile: root.join("sandbox.sb"),
            egress_dir: root.join("egress"),
            scratch_dir: root.join(".work"),
        },
        os_user: "tddy".to_string(),
        login_shell: "/bin/zsh".to_string(),
        subject: "ada".to_string(),
    }
}

/// A jail of `launch` whose runner is a `sleep`; `dir` holds the files its handle names.
pub(super) fn a_sleeping_jail(dir: &Path, launch: &TabJailLaunch) -> Arc<SandboxSessionState> {
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
        dir.join("profile.sb"),
        dir.join("unused.sock"),
        dir.join("sandbox.ready"),
    );
    let (stdout_tx, _) = broadcast::channel(4);
    let (stdin_tx, _) = mpsc::unbounded_channel();
    Arc::new(SandboxSessionState::new(SandboxSessionStateInit {
        pid,
        worktree_path: launch.worktree.clone(),
        stdout_tx,
        capture: Arc::new(StdMutex::new(TerminalCapture::new())),
        stdin_tx,
        ready_marker: launch.paths.ready_marker.clone(),
        handle,
        managed_workflow: None,
    }))
}
