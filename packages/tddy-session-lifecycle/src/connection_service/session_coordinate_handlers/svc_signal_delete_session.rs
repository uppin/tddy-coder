use super::DaemonSessionHost;

use crate::{session_deletion, user_sessions_path::projects_path_for_user};

use tddy_service::proto::session::DeleteSessionResponse;

use tddy_service::proto::session::DeleteSessionRequest;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_rpc::Status;

use tddy_service::proto::session::SignalSessionResponse;

use tddy_rpc::Response;

use tddy_service::proto::session::SignalSessionRequest;

use tddy_rpc::Request;

impl DaemonSessionHost {
    pub(crate) async fn signal_session_at_session_coordinate(
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
            let sessions_base =
                crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            validate_session_id_segment(&req.session_id)
                .map_err(|e| Status::invalid_argument(e.message()))?;

            let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
            let metadata = read_session_metadata(&session_dir)
                .map_err(|_| Status::not_found("session not found"))?;

            let pid = metadata
                .pid
                .ok_or_else(|| Status::failed_precondition("session has no PID"))?;

            log::debug!(
                "SignalSession: resolved pid={} for session={}",
                pid,
                req.session_id
            );

            #[cfg(unix)]
            {
                use tddy_service::proto::session::Signal;

                let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                if !alive {
                    log::debug!("SignalSession: pid={} is not alive", pid);
                    return Err(Status::failed_precondition("process is not alive"));
                }

                let os_signal = match Signal::try_from(req.signal) {
                    Ok(Signal::Sigint) => libc::SIGINT,
                    Ok(Signal::Sigterm) => libc::SIGTERM,
                    Ok(Signal::Sigkill) => libc::SIGKILL,
                    Err(_) => return Err(Status::invalid_argument("invalid signal value")),
                };

                log::info!(
                    "SignalSession: sending signal {} to pid={} session={}",
                    os_signal,
                    pid,
                    req.session_id
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

                Ok(Response::new(SignalSessionResponse {
                    ok: true,
                    message: format!("signal {} sent to pid {}", os_signal, pid),
                }))
            }

            #[cfg(not(unix))]
            {
                let _ = pid;
                Err(Status::unimplemented(
                    "signal delivery is only supported on Unix",
                ))
            }
        }

    pub(crate) async fn delete_session_at_session_coordinate(
            &self,
            request: Request<DeleteSessionRequest>,
        ) -> Result<Response<DeleteSessionResponse>, Status> {
            let req = request.into_inner();
            let session_id = req.session_id.trim();
            if session_id.is_empty() {
                return Err(Status::invalid_argument("session_id is required"));
            }
            log::debug!("DeleteSession: requested session_id={}", session_id);
            let github_user = (self.user_resolver)(&req.session_token)
                .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
            let os_user = &self
                .config
                .os_user_for_github(&github_user)
                .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
            let sessions_base =
                crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            log::debug!(
                "DeleteSession: resolved sessions_base={:?} for os_user={}",
                sessions_base,
                os_user
            );
            let projects_dir_opt = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
            // A split session's worktree lives on another daemon, which must lose it first: deleting
            // this side alone would leave a checkout on a host with no session left to reclaim it. A
            // failure to reach that daemon fails the delete rather than silently dropping its half.
            self.delete_paired_codebase_session(&sessions_base, session_id, &req.session_token)
                .await?;
            // Every clone this session's roster created, on every host that built one — including hosts
            // the operator never looked at. Refused rather than continued if one cannot be reached, for
            // the same reason the paired workspace above is: a delete that succeeded here while a
            // checkout survived elsewhere is exactly the silent leak this is for.
            self.tear_down_every_agent_clone(session_id, &req.session_token)
                .await?;
            // Every admission this session minted is void with the session: a mirror that re-admits
            // after the delete must be refused, and `revoke_all_for_session` is the bulk revocation
            // that does it. (Per-daemon revocation on the last detach is in `tear_down_agent_clone`;
            // this is the session-wide sweep that catches admissions whose clones were already gone.)
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
            session_deletion::close_session_room(&self.session_rooms, session_id);
            session_deletion::delete_session_directory(
                &sessions_base,
                session_id,
                projects_dir_opt.as_deref(),
            )?;
            log::info!("DeleteSession: successfully removed session {}", session_id);
            Ok(Response::new(DeleteSessionResponse { ok: true }))
        }
}
