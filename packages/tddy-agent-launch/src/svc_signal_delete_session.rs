use tddy_cli_sessions::tmux::{agent_tmux_session_name, kill_tmux_session};
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_session_activity::session_deletion;

use tddy_service::proto::session::DeleteSessionResponse;

use tddy_service::proto::session::DeleteSessionRequest;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_rpc::Status;

use tddy_service::proto::session::SignalSessionResponse;

use tddy_rpc::Response;

use tddy_service::proto::session::SignalSessionRequest;

use std::path::Path;

use crate::launch_ports::LaunchSessions;
use tddy_rpc::Request;

impl LaunchSessions {
    pub async fn signal_session_at_session_coordinate(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "SignalSession: session_id={}, signal={}",
            req.session_id,
            req.signal
        );

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let message = signal_session_dir(&session_dir, &req.session_id, req.signal)?;
        Ok(Response::new(SignalSessionResponse { ok: true, message }))
    }

    pub async fn delete_session_at_session_coordinate(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        log::debug!("DeleteSession: requested session_id={}", session_id);
        let os_user = self.os_user_of_session_token(&req.session_token)?;
        self.delete_session_as(&os_user, session_id, &req.session_token)
            .await
    }

    /// The OS user a session token's caller is mapped to.
    pub(crate) fn os_user_of_session_token(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    /// The teardown [`Self::delete_session_at_session_coordinate`] runs once the caller is resolved
    /// to `os_user`, so a start that carried no session token (an operator start) can remove what it
    /// created by its OS user.
    ///
    /// `session_token` is the credential the session's start carried. The split codebase half and the
    /// agent clones are deleted on the peers that hold them, and a peer authenticates that delete by
    /// this token: a start that carried none gets its peer-held parts refused, never skipped.
    pub(crate) async fn delete_session_as(
        &self,
        os_user: &str,
        session_id: &str,
        session_token: &str,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        log::debug!(
            "DeleteSession: resolved sessions_base={:?} for os_user={}",
            sessions_base,
            os_user
        );
        // Every part a peer holds is checked before anything here is torn down: a delete one of
        // them would refuse is refused with the session and its agent still running.
        self.refuse_a_delete_a_peer_would_refuse(&sessions_base, session_id)
            .await?;
        // The agent of a tmux-run session lives in its tmux session, so that is killed before
        // anything else of the session is torn down: a refused kill refuses the delete with the
        // session intact, and a retry kills whatever is left.
        kill_recorded_tmux_session(
            os_user,
            session_id,
            &unified_session_dir_path(&sessions_base, session_id),
        )
        .await?;
        // A worktree kept after the delete (`keep_worktree_on_delete`) must not keep pointing a
        // shell-run `tddy-tools` at a session that is gone.
        remove_session_pointer(session_id, &sessions_base).await;
        let projects_dir_opt = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
        // A split session's worktree lives on another daemon, which must lose it first: deleting
        // this side alone would leave a checkout on a host with no session left to reclaim it. A
        // failure to reach that daemon fails the delete rather than silently dropping its half.
        self.split_sessions
            .delete_paired_codebase_session(&sessions_base, session_id, session_token)
            .await?;
        // Every clone this session's roster created, on every host that built one — including hosts
        // the operator never looked at. Refused rather than continued if one cannot be reached, for
        // the same reason the paired workspace above is: a delete that succeeded here while a
        // checkout survived elsewhere is exactly the silent leak this is for.
        self.agent_roster
            .tear_down_every_agent_clone(session_id, session_token)
            .await?;
        // Every admission this session minted is void with the session: a mirror that re-admits
        // after the delete must be refused, and `revoke_all_for_session` is the bulk revocation
        // that does it. (Per-daemon revocation on the last detach is in `tear_down_agent_clone`;
        // this is the session-wide sweep that catches admissions whose clones were already gone.)
        self.forget_session_state(session_id).await;
        session_deletion::close_session_room(&*self.session_rooms, session_id);
        session_deletion::delete_session_directory(
            &sessions_base,
            session_id,
            projects_dir_opt.as_deref(),
            Some(self.coding_agents.agents()),
        )?;
        log::info!("DeleteSession: successfully removed session {}", session_id);
        Ok(Response::new(DeleteSessionResponse { ok: true }))
    }

    async fn forget_session_state(&self, session_id: &str) {
        let revoked = self.session_admissions.revoke_all_for_session(session_id);
        if revoked > 0 {
            log::info!("revoked {revoked} admission(s) for session {session_id} on session delete");
        }
        // The other direction: this daemon may be *holding* a clone, whose workspace session is the
        // one being deleted. Forgetting it before the directory goes is what stops a tool call
        // arriving a moment later from being served out of a checkout that no longer exists.
        self.hosted_agent_clones.forget_checkout(session_id);
        // The conversation this daemon was tailing goes with the session: its consumer task exits on
        // the next record rather than holding a subscription for the daemon's life, and a session id
        // reused later starts from nothing observed instead of the deleted session's last call.
        self.session_agent_inference.forget(session_id);
        // Its tool session (if it ran one) stops being answered on its user's host-session socket:
        // a token request naming it is refused from here on, and the daemon stops holding the
        // start's session token for it.
        self.host_session_sockets.registry().unregister(session_id);
        if let Some(sandbox) = self.sandbox_manager.get(session_id).await {
            sandbox.stop();
        }
        let _ = self.sandbox_manager.remove(session_id).await;
        // The workspace jail goes the same way, and *before* the directory does: the jail is a live
        // process holding the worktree open, so a jail left registered would go on running against
        // a checkout that no longer exists — for the rest of this daemon's life, since the registry
        // is the only thing holding it. Dropping the last handle stops it.
        if let Some(jail) = self.workspace_sandboxes.remove(session_id).await {
            jail.stop();
        }
    }

    /// Refuses a delete of `session_id` that one of its peer-held parts would refuse, before
    /// [`Self::delete_session_as`] tears anything down: a codebase half on another daemon
    /// (`SplitSessions::refuse_an_unreachable_paired_codebase`) or
    /// an agent clone (`AgentRoster::refuse_an_unreachable_clone_host`) while
    /// this daemon cannot reach the common room. What only the peer can answer — reachable now,
    /// failing at the delete — still fails after the local teardown began (`#agent-tools 30/32`).
    pub(crate) async fn refuse_a_delete_a_peer_would_refuse(
        &self,
        sessions_base: &Path,
        session_id: &str,
    ) -> Result<(), Status> {
        self.split_sessions
            .refuse_an_unreachable_paired_codebase(sessions_base, session_id)
            .await?;
        self.agent_roster
            .refuse_an_unreachable_clone_host(session_id)
            .await?;
        Ok(())
    }
}

async fn remove_session_pointer(session_id: &str, sessions_base: &std::path::Path) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    if let Err(e) = tokio::task::spawn_blocking(move || {
        remove_recorded_session_pointer(&session_dir, Path::new("ssh"))
    })
    .await
    {
        log::warn!("the session pointer's removal did not finish: {e}");
    }
}

/// Remove the session pointer ([`crate::cli_session_pointer`]) from the worktree the session in
/// `session_dir` records, if it records one — on the ssh target it records
/// (`ssh_config_host`, reached through the ssh client `ssh`) when it records one.
pub(crate) fn remove_recorded_session_pointer(session_dir: &Path, ssh: &Path) {
    let Ok(metadata) = read_session_metadata(session_dir) else {
        return;
    };
    if let Some(worktree) = metadata.repo_path {
        crate::cli_session_pointer::remove_session_pointer(
            Path::new(&worktree),
            &crate::cli_session_pointer::PointerHost::through(
                ssh,
                metadata.ssh_config_host.as_deref(),
            ),
        );
    }
}

/// Kill the tmux session that a tmux-run session's agent runs in, as the OS user the session runs as,
/// before the session is deleted. A session that records no tmux session has nothing to kill, and
/// killing a tmux session that is already gone is not an error.
///
/// When `.session.yaml` cannot be read the recorded name is not available, but the name is derived
/// from the session id ([`agent_tmux_session_name`]), so that name is killed: the session may have
/// been tmux-run, and a tmux session left behind would keep its agent running with nothing to
/// reclaim it. A host where tmux cannot be run at all is logged and does not block deleting the
/// broken session, since a session that was never tmux-run cannot have a tmux session to leak.
async fn kill_recorded_tmux_session(
    os_user: &str,
    session_id: &str,
    session_dir: &Path,
) -> Result<(), Status> {
    if !session_dir.is_dir() {
        // Nothing of this session is on this daemon: the directory check in the delete refuses it.
        return Ok(());
    }
    let (name, recorded) = match read_session_metadata(session_dir) {
        Ok(metadata) => match metadata.tmux_session {
            Some(name) => (name, true),
            None => return Ok(()),
        },
        Err(error) => {
            log::warn!(
                "DeleteSession: no readable .session.yaml in {}, killing the tmux session named by the id: {error}",
                session_dir.display()
            );
            (agent_tmux_session_name(session_id), false)
        }
    };
    let os_user = os_user.to_string();
    let killed = tokio::task::spawn_blocking(move || kill_tmux_session(&os_user, &name))
        .await
        .map_err(|e| Status::internal(format!("the tmux kill did not finish: {e}")))?;
    match killed {
        Ok(()) => Ok(()),
        Err(error) if recorded => Err(Status::internal(format!(
            "could not kill the session's tmux session: {error}"
        ))),
        Err(error) => {
            log::warn!(
                "DeleteSession: could not look for the tmux session of {session_id}, whose metadata is unreadable: {error}"
            );
            Ok(())
        }
    }
}

/// Sends `signal` (a `Signal` proto value) to the process the session in `session_dir` records, as
/// `SignalSession` does: refused when the session records no PID or its process is not alive.
/// The project-home `TerminateSession` delivers SIGTERM through the same path (`#start-parity` 7/9).
pub fn signal_session_dir(
    session_dir: &Path,
    session_id: &str,
    signal: i32,
) -> Result<String, Status> {
    let metadata =
        read_session_metadata(session_dir).map_err(|_| Status::not_found("session not found"))?;

    let pid = metadata
        .pid
        .ok_or_else(|| Status::failed_precondition("session has no PID"))?;

    log::debug!(
        "SignalSession: resolved pid={} for session={}",
        pid,
        session_id
    );

    #[cfg(unix)]
    {
        use tddy_service::proto::session::Signal;

        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if !alive {
            log::debug!("SignalSession: pid={} is not alive", pid);
            return Err(Status::failed_precondition("process is not alive"));
        }

        let os_signal = match Signal::try_from(signal) {
            Ok(Signal::Sigint) => libc::SIGINT,
            Ok(Signal::Sigterm) => libc::SIGTERM,
            Ok(Signal::Sigkill) => libc::SIGKILL,
            Err(_) => return Err(Status::invalid_argument("invalid signal value")),
        };

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
