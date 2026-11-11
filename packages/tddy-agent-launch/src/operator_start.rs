//! The start of a session for an operator the daemon already identified by their OS user — what a
//! satellite's session-starter port (`tddy_daemon_extensions::SessionStarter`) reaches
//! (`#agent-tools 18/19`).
//!
//! The same start [`LaunchSessions::start_session_core`] runs, minus the one step a satellite
//! cannot take: resolving a session token to a signed-in user. The satellite authenticated its
//! operator its own way (Telegram: the chat's linked GitHub login, mapped to an OS user), so no
//! token is minted inside the daemon and none reaches the agent's environment. Everything after the
//! identity — the harness profile, the tmux check, the local placement, the spec launch, the
//! branch-conflict guard — is the generic start's own.

use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;

use tddy_task::CancellationToken;

use crate::launch_ports::LaunchSessions;
use crate::placement::classify_start_placement;

impl LaunchSessions {
    /// Start `req` as `os_user`, an operator a satellite identified. Refused like
    /// [`Self::start_session_core`]: a requested role, a conflicting tmux run, a host without coding
    /// agents (`FAILED_PRECONDITION`); and refused `INVALID_ARGUMENT` when `req` names a session
    /// token (an operator start carries none), a peer daemon (it starts here), or a part a peer
    /// would hold ([`refuse_peer_held_parts`]).
    pub async fn start_session_for_os_user(
        &self,
        os_user: &str,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if !req.session_token.trim().is_empty() {
            return Err(Status::invalid_argument(
                "an operator start is keyed by its OS user and carries no session token",
            ));
        }
        if !req.daemon_instance_id.trim().is_empty() {
            return Err(Status::invalid_argument(
                "an operator start runs on this daemon and may not name a peer daemon",
            ));
        }
        refuse_peer_held_parts(&req)?;
        crate::harness_profile_request::refuse_a_requested_role(&req)?;
        let req = self.apply_harness_profile(req)?;
        crate::start_request_checks::refuse_a_conflicting_tmux_run_start(
            &req,
            Some(&*self.coding_agents),
        )?;
        let (local_id, eligible_ids) = self.eligible_daemon_ids();
        let placement =
            classify_start_placement(&req, Some(&*self.coding_agents), local_id, eligible_ids)?;
        self.start_here(
            req,
            os_user,
            None,
            placement,
            progress,
            &CancellationToken::new(),
        )
        .await
    }
}

/// Refuses `INVALID_ARGUMENT`, naming the field, an operator start that asks for a part a peer would
/// hold: a codebase half on another daemon (`codebase_daemon_instance_id`), a jailed checkout
/// deleted as a session of its own (`sandboxed_codebase`), or a seeded roster
/// (`specialized_agents`, `clone_placed_agents`). An operator start's undo is keyed by its OS user,
/// and a peer authenticates a delete by a session token the operator start does not carry, so it
/// may not create what it could not remove (`#agent-tools 30/32`).
pub(crate) fn refuse_peer_held_parts(req: &StartSessionRequest) -> Result<(), Status> {
    if !req.codebase_daemon_instance_id.trim().is_empty() {
        return Err(Status::invalid_argument(
            "an operator start may not set codebase_daemon_instance_id: its codebase half would be held by a peer",
        ));
    }
    if req.sandboxed_codebase {
        return Err(Status::invalid_argument(
            "an operator start may not set sandboxed_codebase: its jailed checkout would be held as a session of its own",
        ));
    }
    if !req.specialized_agents.is_empty() {
        return Err(Status::invalid_argument(
            "an operator start may not set specialized_agents: its seeded roster would be held by a peer",
        ));
    }
    if !req.clone_placed_agents.is_empty() {
        return Err(Status::invalid_argument(
            "an operator start may not set clone_placed_agents: its agents would be placed on clones held by a peer",
        ));
    }
    Ok(())
}
