//! A spawned tool's end, reported by the backend that holds it (`#session-liveness` 3/4).
//!
//! Every tool spawn returns a [`SpawnedTool`]: the [`SpawnResult`] it always returned, and an
//! [`ExitReceiver`] that resolves once the child has ended. Who reports it depends on who holds the
//! child:
//!
//! - **direct spawn** ([`spawn_tool_as_user`]) — the wait thread `spawn_as_user` already runs sends
//!   the status instead of only logging it;
//! - **forked spawn worker** ([`crate::spawn_worker::SpawnClient::spawn_tool`]) — the worker's own
//!   wait thread writes [`crate::spawn_worker::WorkerEvent::Exited`] on the request's event stream;
//! - **supervisor** ([`spawn_tool_via_supervisor`]) — the daemon polls `SessionStatus`
//!   ([`watch_supervised_exit`]). The supervisor reaps its own children, so its `EXITED` is exact:
//!   this is not a pid probe.
//!
//! The plumbing lives here rather than in `spawner.rs` / `spawn_worker.rs`, which are over the file
//! budget (`docs/dev/todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_supervisor::request::SessionStatus;

pub use tddy_task::child_exit::{exit_channel, ChildExit, ExitReceiver, ExitSender, HolderGone};

use crate::spawn_worker::{SpawnClient, SpawnRequest};
use crate::spawner::{LiveKitCreds, SpawnOptions, SpawnResult, StartupWatch};

/// A tool child that outlived its startup watch, and the report of its end.
#[derive(Debug)]
pub struct SpawnedTool {
    pub result: SpawnResult,
    pub exit: ExitReceiver,
}

/// [`crate::spawner::spawn_as_user`], keeping the end of the child its wait thread reaps.
#[cfg(unix)]
#[allow(clippy::too_many_arguments)] // Mirrors `spawner::spawn_as_user`, which it wraps.
pub fn spawn_tool_as_user(
    os_user: &str,
    tool_path: &str,
    tddy_data_dir: &Path,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    opts: SpawnOptions<'_>,
    child_log_level: &str,
    child_log_format: &str,
    coder_log_config_yaml: Option<&str>,
    startup: StartupWatch,
) -> anyhow::Result<SpawnedTool> {
    // TODO(session-liveness 3/4): implemented in the green step.
    let _ = (
        os_user,
        tool_path,
        tddy_data_dir,
        repo_path,
        livekit,
        opts,
        child_log_level,
        child_log_format,
        coder_log_config_yaml,
        startup,
    );
    todo!("spawn as the user and send the wait thread's status on an exit channel")
}

impl SpawnClient {
    /// [`SpawnClient::spawn`], keeping the [`crate::spawn_worker::WorkerEvent::Exited`] the worker
    /// writes for this request once its child ends.
    pub fn spawn_tool(&self, req: SpawnRequest) -> anyhow::Result<SpawnedTool> {
        // TODO(session-liveness 3/4): implemented in the green step.
        let _ = req;
        todo!("spawn through the worker and route its Exited event to an exit channel")
    }
}

/// [`crate::supervisor_spawn::spawn_session_via_supervisor`], with the end of the child the
/// supervisor holds watched by [`watch_supervised_exit`].
#[cfg(unix)]
pub async fn spawn_tool_via_supervisor(
    socket_path: &Path,
    req: &SpawnRequest,
) -> anyhow::Result<SpawnedTool> {
    // TODO(session-liveness 3/4): implemented in the green step.
    let _ = (socket_path, req);
    todo!("spawn through the supervisor and watch its SessionStatus for the exit")
}

/// Poll the supervisor's `SessionStatus` for `pid` every `interval` until it reports the session
/// exited, and resolve the returned receiver with [`exit_of`] that status.
pub fn watch_supervised_exit(socket_path: PathBuf, pid: u32, interval: Duration) -> ExitReceiver {
    // TODO(session-liveness 3/4): implemented in the green step.
    let _ = (socket_path, pid, interval);
    todo!("poll SessionStatus until EXITED")
}

/// The end a supervisor `SessionStatus` reports: `None` while the session runs. The supervisor
/// reports an exit code, or none when a signal killed the child — it does not say which signal.
pub fn exit_of(status: &SessionStatus) -> Option<ChildExit> {
    // TODO(session-liveness 3/4): implemented in the green step.
    let _ = status;
    todo!("map EXITED{{code}} to a ChildExit")
}

#[cfg(test)]
mod tests {
    use tddy_supervisor::request::{SessionState, SessionStatus};

    use super::*;

    fn a_status(state: SessionState, exit_code: Option<i32>) -> SessionStatus {
        SessionStatus {
            pid: 4242,
            state,
            exit_code,
        }
    }

    #[test]
    fn a_supervised_session_that_exited_with_a_code_ended_with_that_code() {
        // Given
        let status = a_status(SessionState::Exited, Some(3));

        // When
        let exit = exit_of(&status);

        // Then
        assert_eq!(exit, Some(ChildExit::with_code(3)));
    }

    #[test]
    fn a_supervised_session_killed_by_a_signal_ended_with_neither_code_nor_signal() {
        // Given
        let status = a_status(SessionState::Exited, None);

        // When
        let exit = exit_of(&status);

        // Then
        assert_eq!(
            exit,
            Some(ChildExit {
                code: None,
                signal: None
            })
        );
    }

    #[test]
    fn a_running_supervised_session_has_not_ended() {
        // Given
        let status = a_status(SessionState::Running, None);

        // When
        let exit = exit_of(&status);

        // Then
        assert_eq!(exit, None);
    }
}
