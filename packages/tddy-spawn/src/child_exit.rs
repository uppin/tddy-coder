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

use tddy_supervisor::request::{SessionState, SessionStatus};

pub use tddy_task::child_exit::{exit_channel, ChildExit, ExitReceiver, ExitSender, HolderGone};

use crate::spawn_worker::{SpawnClient, SpawnRequest, WorkerRequest, WorkerResponse};
use crate::spawner::{self, LiveKitCreds, SpawnOptions, SpawnResult, StartupWatch};

/// How often the daemon asks the supervisor whether a tool child it holds has ended.
const SUPERVISOR_EXIT_POLL: Duration = Duration::from_millis(500);

/// A tool child that outlived its startup watch, and the report of its end.
#[derive(Debug)]
pub struct SpawnedTool {
    pub result: SpawnResult,
    pub exit: ExitReceiver,
}

/// What a spawn's wait thread calls, once, with the child's pid and how it ended.
pub(crate) type OnExit = Box<dyn FnOnce(u32, ChildExit) + Send>;

/// Hand `on_exit`, if there is one, the end `waited` reports. A wait that failed leaves the holder
/// knowing only that the child is gone, which is reported as an end with neither code nor signal.
fn report_wait(
    on_exit: Option<OnExit>,
    pid: u32,
    waited: &std::io::Result<std::process::ExitStatus>,
) {
    let Some(on_exit) = on_exit else { return };
    let exit = match waited {
        Ok(status) => of_status(status),
        Err(_) => ChildExit {
            code: None,
            signal: None,
        },
    };
    on_exit(pid, exit);
}

/// Reap `child` on a thread of its own, log how it ended, and hand that end to `on_exit`. The
/// thread is the child's only waiter, so its wait is what makes the end exact.
pub(crate) fn reap_in_background(
    mut child: std::process::Child,
    session_id: String,
    pid: u32,
    on_exit: Option<OnExit>,
) {
    std::thread::spawn(move || {
        let waited = child.wait();
        match &waited {
            Ok(status) => {
                log::info!("child exited session_id={session_id} pid={pid} status={status}")
            }
            Err(e) => log::warn!("child wait failed session_id={session_id} pid={pid} err={e}"),
        }
        report_wait(on_exit, pid, &waited);
    });
}

#[cfg(unix)]
fn of_status(status: &std::process::ExitStatus) -> ChildExit {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => ChildExit::with_code(code),
        (None, Some(signal)) => ChildExit::by_signal(signal),
        (None, None) => ChildExit {
            code: None,
            signal: None,
        },
    }
}

#[cfg(not(unix))]
fn of_status(status: &std::process::ExitStatus) -> ChildExit {
    ChildExit {
        code: status.code(),
        signal: None,
    }
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
    let (holder, exit) = exit_channel();
    let result = spawner::spawn_as_user_reporting(
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
        Some(Box::new(move |_pid, end| holder.send(end))),
    )?;
    Ok(SpawnedTool { result, exit })
}

impl SpawnClient {
    /// [`SpawnClient::spawn`], keeping the [`crate::spawn_worker::WorkerEvent::Exited`] the worker
    /// writes for this request once its child ends.
    pub fn spawn_tool(&self, req: SpawnRequest) -> anyhow::Result<SpawnedTool> {
        let (holder, exit) = exit_channel();
        match self.request(WorkerRequest::Spawn(Box::new(req)), |_| {}, Some(holder))? {
            WorkerResponse::SpawnOk { result } => Ok(SpawnedTool { result, exit }),
            WorkerResponse::Error { message } => Err(anyhow::anyhow!("{}", message)),
            WorkerResponse::CloneOk => Err(anyhow::anyhow!("unexpected clone_ok for spawn")),
        }
    }
}

/// [`crate::supervisor_spawn::spawn_session_via_supervisor`], with the end of the child the
/// supervisor holds watched by [`watch_supervised_exit`].
#[cfg(unix)]
pub async fn spawn_tool_via_supervisor(
    socket_path: &Path,
    req: &SpawnRequest,
) -> anyhow::Result<SpawnedTool> {
    let result = crate::supervisor_spawn::spawn_session_via_supervisor(socket_path, req).await?;
    let exit = watch_supervised_exit(socket_path.to_path_buf(), result.pid, SUPERVISOR_EXIT_POLL);
    Ok(SpawnedTool { result, exit })
}

/// Poll the supervisor's `SessionStatus` for `pid` every `interval` until it reports the session
/// exited, and resolve the returned receiver with [`exit_of`] that status.
///
/// A supervisor that cannot be reached, or that no longer knows the pid, leaves the daemon with no
/// knowledge of the child: the receiver then resolves as [`HolderGone`] rather than guessing.
pub fn watch_supervised_exit(socket_path: PathBuf, pid: u32, interval: Duration) -> ExitReceiver {
    let (holder, exit) = exit_channel();
    tokio::spawn(async move {
        let client = match crate::supervisor_client::connect_supervisor(&socket_path).await {
            Ok(client) => client,
            Err(e) => {
                log::warn!("supervised exit: cannot reach the supervisor for pid={pid}: {e}");
                return;
            }
        };
        loop {
            match client.session_status(pid).await {
                Ok(status) => {
                    if let Some(end) = exit_of(&status) {
                        holder.send(end);
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("supervised exit: the supervisor cannot report pid={pid}: {e}");
                    return;
                }
            }
            tokio::time::sleep(interval).await;
        }
    });
    exit
}

/// The end a supervisor `SessionStatus` reports: `None` while the session runs. The supervisor
/// reports an exit code, or none when a signal killed the child — it does not say which signal.
pub fn exit_of(status: &SessionStatus) -> Option<ChildExit> {
    match status.state {
        SessionState::Running => None,
        SessionState::Exited => Some(ChildExit {
            code: status.exit_code,
            signal: None,
        }),
    }
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
