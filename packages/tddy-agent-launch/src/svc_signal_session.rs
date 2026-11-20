use crate::launch_ports::LaunchSessions;
use std::path::Path;
use tddy_core::read_session_metadata;
use tddy_rpc::Status;

/// The pid of the main agent `cli` or `sandboxes` hold for `session_id`: its main terminal's
/// process, or its sandbox runner's. `None` when no agent is held — nothing is read off the
/// session's `.session.yaml`, whose recorded pid the OS may have reissued.
pub(crate) async fn held_agent_pid(
    cli: &tddy_cli_sessions::cli_session_manager::CliSessionManager,
    sandboxes: &tddy_daemon_sandbox::sandbox_session::SandboxSessionManager,
    session_id: &str,
) -> Option<u32> {
    if let Some(main) = cli.get(session_id).await {
        return Some(main.pid);
    }
    sandboxes
        .get(session_id)
        .await
        .and_then(|sandbox| sandbox.runner_pid())
}

/// Sends `signal` (a `Signal` proto value) to the main agent the daemon holds for the session in
/// `session_dir`, as `SignalSession` does: refused with `FAILED_PRECONDITION`, and nothing
/// signalled, when no agent is held. The project-home `TerminateSession` delivers SIGTERM through
/// the same path (`#start-parity` 7/9).
pub async fn signal_held_session(
    cli: &tddy_cli_sessions::cli_session_manager::CliSessionManager,
    sandboxes: &tddy_daemon_sandbox::sandbox_session::SandboxSessionManager,
    session_dir: &Path,
    session_id: &str,
    signal: i32,
) -> Result<String, Status> {
    read_session_metadata(session_dir).map_err(|_| Status::not_found("session not found"))?;
    // A tool session's `tddy-coder` is held by its spawn backend as a process tab: it is signalled
    // through that tab, never through the pid the session's record names.
    if cli.process_tab(session_id).await.is_some() {
        return signal_process_tab(cli, session_id, signal).await;
    }
    let pid = held_agent_pid(cli, sandboxes, session_id)
        .await
        .ok_or_else(|| Status::failed_precondition("session has no running agent"))?;
    deliver_signal(pid, session_id, signal)
}

/// Delivers `signal` (a `Signal` proto value) to the process tab `cli` holds for `session_id`,
/// through the holder of its child.
pub(crate) async fn signal_process_tab(
    cli: &tddy_cli_sessions::cli_session_manager::CliSessionManager,
    session_id: &str,
    signal: i32,
) -> Result<String, Status> {
    use tddy_cli_sessions::cli_session_manager::SignalRefused;

    let os_signal = os_signal_of(signal)?;
    log::info!(
        "SignalSession: sending signal {} to the process tab of session={}",
        os_signal,
        session_id
    );
    cli.signal_process_tab(session_id, os_signal)
        .await
        .map(|()| format!("signal {os_signal} sent to the process tab of {session_id}"))
        .map_err(|refused| match refused {
            SignalRefused::NotHeld => Status::failed_precondition("session has no running agent"),
            SignalRefused::Holder(error) => {
                Status::internal(format!("failed to send signal: {error}"))
            }
        })
}

/// The OS signal a `Signal` proto value names.
pub(crate) fn os_signal_of(signal: i32) -> Result<i32, Status> {
    use tddy_service::proto::session::Signal;

    match Signal::try_from(signal) {
        Ok(Signal::Sigint) => Ok(libc::SIGINT),
        Ok(Signal::Sigterm) => Ok(libc::SIGTERM),
        Ok(Signal::Sigkill) => Ok(libc::SIGKILL),
        Err(_) => Err(Status::invalid_argument("invalid signal value")),
    }
}

/// Delivers `signal` to `pid`, refused when that process is not alive.
pub(crate) fn deliver_signal(pid: u32, session_id: &str, signal: i32) -> Result<String, Status> {
    log::debug!(
        "SignalSession: resolved pid={} for session={}",
        pid,
        session_id
    );

    #[cfg(unix)]
    {
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if !alive {
            log::debug!("SignalSession: pid={} is not alive", pid);
            return Err(Status::failed_precondition("process is not alive"));
        }

        let os_signal = os_signal_of(signal)?;

        log::info!(
            "SignalSession: sending signal {} to pid={} session={}",
            os_signal,
            pid,
            session_id
        );

        let ret = unsafe { libc::kill(pid as i32, os_signal) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            log::error!(
                "SignalSession: kill({}, {}) failed: {}",
                pid,
                os_signal,
                err
            );
            return Err(Status::internal(format!("failed to send signal: {}", err)));
        }

        Ok(format!("signal {} sent to pid {}", os_signal, pid))
    }

    #[cfg(not(unix))]
    {
        let _ = (pid, signal, session_id);
        Err(Status::unimplemented(
            "signal delivery is only supported on Unix",
        ))
    }
}
impl LaunchSessions {
    /// The sessions this daemon holds a live agent for: its terminals' and its sandboxed runners'.
    /// The one live set every listing, the ensure-home check and the idle reaper read.
    pub async fn live_sessions(&self) -> tddy_daemon_kernel::live_sessions::LiveSessions {
        self.cli_manager
            .live_agent_sessions()
            .await
            .union(self.sandbox_manager.live_sessions().await)
    }

    /// Sends `signal` to the main agent this daemon holds for the session in `session_dir`; see
    /// [`signal_held_session`].
    pub async fn signal_session_dir(
        &self,
        session_dir: &Path,
        session_id: &str,
        signal: i32,
    ) -> Result<String, Status> {
        signal_held_session(
            &self.cli_manager,
            &self.sandbox_manager,
            session_dir,
            session_id,
            signal,
        )
        .await
    }
}
