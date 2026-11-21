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
        let message = self
            .signal_session_dir(&session_dir, &req.session_id, req.signal)
            .await?;
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
        let sessions_base = self.sessions_base_for_delete(os_user)?;
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
        // A target session's hook log is followed from here: stop its follower and remove the
        // log on its target before its record and worktree go.
        self.remote_hook_followers
            .stop_and_remove_log(session_id)
            .await;
        // A target session's worktree is on its target, where the local deletion below cannot
        // reach: what to remove there is captured now, while its record is still on disk.
        let target_worktree =
            target_worktree_of(&unified_session_dir_path(&sessions_base, session_id));
        session_deletion::delete_session_directory(
            &sessions_base,
            session_id,
            projects_dir_opt.as_deref(),
            Some(self.coding_agents.agents()),
        )?;
        if let Some((target, worktree)) = &target_worktree {
            // The shell is resolved with the caller's account, as the start resolves it, so a
            // Coder target's worktree is removed on its workspace too.
            let subject = (self.user_resolver)(session_token).unwrap_or_default();
            let shell = crate::opencode_cli_spawn::session_start::shell_for_session_target(
                target,
                &subject,
                self.coder_credentials.as_ref(),
            )?;
            remove_target_worktree(shell.as_ref(), worktree).await?;
        }
        log::info!("DeleteSession: successfully removed session {}", session_id);
        Ok(Response::new(DeleteSessionResponse { ok: true }))
    }

    fn sessions_base_for_delete(&self, os_user: &str) -> Result<std::path::PathBuf, Status> {
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
        Ok(sessions_base)
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
        // Every terminal of the session — its main agent and each tab — is stopped with it: the
        // recorded pid is not signalled, since nothing guarantees it still names this session.
        self.cli_manager.stop_session(session_id).await;
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
    remove_recorded_session_pointer(&session_dir).await;
}

/// The pointer host a session's record names: its target when it has one (the agent ran there),
/// else its ssh exec host, else this host.
fn pointer_host_of(
    metadata: &tddy_core::SessionMetadata,
) -> crate::cli_session_pointer::PointerHost {
    match crate::remote_agent_launch::target_config_host(metadata.target.as_ref()) {
        Some(alias) => crate::cli_session_pointer::PointerHost::of(Some(alias)),
        None => crate::cli_session_pointer::PointerHost::of(metadata.ssh_config_host.as_deref()),
    }
}

/// Remove the session pointer ([`crate::cli_session_pointer`]) from the worktree the session in
/// `session_dir` records, if it records one — on the target it records (its session `target`, or
/// its `ssh_config_host`, reached through the ssh client `ssh`) when it records one.
pub(crate) async fn remove_recorded_session_pointer(session_dir: &Path) {
    let Ok(metadata) = read_session_metadata(session_dir) else {
        return;
    };
    if let Some(worktree) = &metadata.repo_path {
        crate::cli_session_pointer::remove_session_pointer(
            Path::new(worktree),
            &pointer_host_of(&metadata),
        )
        .await;
    }
}

/// What a delete removes on a session's target, captured from its record before the record
/// goes: the target and the worktree there. `None` for a session whose agent ran here.
pub(crate) fn target_worktree_of(session_dir: &Path) -> Option<(tddy_core::SessionTarget, String)> {
    let metadata = read_session_metadata(session_dir).ok()?;
    Some((metadata.target?, metadata.repo_path?))
}

/// The clone a target worktree belongs to: `<root>/.worktrees/<session>` → `<root>`. `None` for a
/// path that is not a tddy worktree path.
pub(crate) fn target_checkout_root_of(worktree: &str) -> Option<String> {
    let path = Path::new(worktree);
    let worktrees = path.parent()?;
    (worktrees.file_name()?.to_str()? == ".worktrees")
        .then(|| worktrees.parent())
        .flatten()
        .map(|root| root.to_string_lossy().into_owned())
}

/// Remove a target session's worktree on its target, through `shell`: the session is deleted, so
/// nothing of it is left there — the worktree, the pointer inside it, and the clone's registration
/// of it (`git worktree remove`, then `prune`), so the clone does not keep a dangling entry.
pub(crate) async fn remove_target_worktree(
    shell: &dyn tddy_remote_shell::RemoteShell,
    worktree: &str,
) -> Result<(), Status> {
    let root = target_checkout_root_of(worktree).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the target worktree {worktree} is not under a clone's .worktrees directory"
        ))
    })?;
    for command in [
        format!(
            "git -C {} worktree remove --force {}",
            tddy_core::ssh_exec::shell_single_quote(&root),
            tddy_core::ssh_exec::shell_single_quote(worktree),
        ),
        format!(
            "git -C {} worktree prune",
            tddy_core::ssh_exec::shell_single_quote(&root),
        ),
    ] {
        shell
            .exec(tddy_remote_shell::ExecRequest::new(command))
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .success_or_exited()
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    Ok(())
}

/// [`remove_recorded_session_pointer`], reaching an ssh target through the ssh client `ssh`.
#[allow(dead_code)]
pub(crate) async fn remove_recorded_session_pointer_through(session_dir: &Path, ssh: &Path) {
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
        )
        .await;
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

pub use crate::svc_signal_session::signal_held_session;

#[cfg(test)]
mod target_worktree_removal_tests {
    //! `#remote-shell` 10/11: a deleted target session leaves nothing on its target.

    use pretty_assertions::assert_eq;
    use tddy_remote_shell::{ExecOutput, InMemoryShell};

    use super::{remove_target_worktree, target_checkout_root_of};

    const ROOT: &str = "/home/coder/.tddy/repos/qape-hq";
    const WORKTREE: &str = "/home/coder/.tddy/repos/qape-hq/.worktrees/01a1b2c3";

    #[test]
    fn a_worktrees_clone_is_two_directories_up() {
        assert_eq!(
            (
                target_checkout_root_of(WORKTREE),
                target_checkout_root_of("/home/coder/somewhere/else"),
            ),
            (Some(ROOT.to_string()), None)
        );
    }

    #[tokio::test]
    async fn removing_a_target_worktree_unregisters_it_from_the_clone() {
        // Given a target that answers every command with success
        let target = InMemoryShell::new("coder.remote-dev");
        target.answers_when(|_| Some(ExecOutput::succeeded(Vec::new())));

        // When
        remove_target_worktree(&target, WORKTREE)
            .await
            .expect("the removal succeeds");

        // Then git removes and prunes the worktree in its clone; nothing is merely `rm -rf`ed
        let run = target.commands_run().join("\n");
        assert!(
            run.contains(&format!(
                "git -C '{ROOT}' worktree remove --force '{WORKTREE}'"
            )) && run.contains(&format!("git -C '{ROOT}' worktree prune"))
                && !run.contains("rm -rf"),
            "the clone is told the worktree is gone:\n{run}"
        );
    }
}
