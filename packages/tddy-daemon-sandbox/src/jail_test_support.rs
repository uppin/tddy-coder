//! A jail whose "runner" is a plain process — what every suite of this crate that owns a jail's
//! lifetime builds its jails from. The registries own the runner's lifetime and its exit, which a
//! real process shows and a kernel sandbox adds nothing to.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

use crate::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};

/// A runner that runs until it is stopped.
pub(crate) const SLEEPS: &[&str] = &["sleep", "300"];

/// A runner that exits on its own, with status 3.
pub(crate) const EXITS_3: &[&str] = &["sh", "-c", "exit 3"];

/// A jail whose runner is `runner` (program, then arguments); `dir` holds the files its handle
/// names. The process's stdio is closed: nothing reads a stand-in's terminal.
pub(crate) fn a_jail_running_in(dir: &Path, runner: &[&str]) -> Arc<SandboxSessionState> {
    Arc::new(a_jail_state_in(dir, runner))
}

/// [`a_jail_running_in`], not yet shared: what a state is built further from.
pub(crate) fn a_jail_state_in(dir: &Path, runner: &[&str]) -> SandboxSessionState {
    let (program, args) = runner.split_first().expect("a runner names its program");
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
        dir.join("profile.sb"),
        dir.join("unused.sock"),
        dir.join("sandbox.ready"),
    );
    let (stdout_tx, _) = broadcast::channel(4);
    let (stdin_tx, _) = mpsc::unbounded_channel();
    SandboxSessionState::new(SandboxSessionStateInit {
        pid,
        worktree_path: dir.to_path_buf(),
        stdout_tx,
        capture: Arc::new(StdMutex::new(TerminalCapture::new())),
        stdin_tx,
        ready_marker: dir.join("sandbox.ready"),
        handle,
        managed_workflow: None,
    })
}

/// [`a_jail_running_in`] a directory of its own, returned with it.
pub(crate) fn a_jail_running(runner: &[&str]) -> (Arc<SandboxSessionState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (a_jail_running_in(dir.path(), runner), dir)
}

/// The runner of `jail` is killed by `signal` from outside the daemon, as a crashed or OOM-killed
/// runner is: it dies on its own, not by the daemon's request.
pub(crate) fn the_runner_is_killed_by(jail: &SandboxSessionState, signal: i32) {
    let pid = jail.runner_pid().expect("the stand-in runner is running") as libc::pid_t;
    // SAFETY: `pid` is the stand-in runner this test spawned, still running and unreaped.
    unsafe { libc::kill(pid, signal) };
}
