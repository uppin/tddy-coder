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

use crate::launch_ports::LaunchSessions;

impl LaunchSessions {
    /// Start `req` as `os_user`, an operator a satellite identified. Refused like
    /// [`Self::start_session_core`]: a requested role, a conflicting tmux run, a host without coding
    /// agents (`FAILED_PRECONDITION`); and refused `INVALID_ARGUMENT` when `req` names a session
    /// token (an operator start carries none) or a peer daemon (it starts here).
    pub async fn start_session_for_os_user(
        &self,
        os_user: &str,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // TODO(telegram-spec-launch): implement — `refuse_a_requested_role`,
        // `apply_harness_profile`, `refuse_a_conflicting_tmux_run_start`, then `start_here` with
        // `os_user`, no agent def and a local placement (`classify_start_placement`).
        let _ = (os_user, req, progress);
        todo!("telegram-spec-launch: LaunchSessions::start_session_for_os_user")
    }
}
