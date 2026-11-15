//! `RenameOwnSession`: an agent naming the session it runs in.
//!
//! The caller is `tddy-tools rename-session`, run by the agent inside a daemon-launched claude-cli
//! or cursor-cli session with its credentials taken from the environment the launch exported. It is
//! authenticated exactly as `ReportOperatorNotification` is — the same session-type check, then the
//! same `hook_token` comparison — so an agent can name its own session and no other.
//!
//! The name is held to the rules every writer applies
//! ([`tddy_core::session_display_name::normalize_display_name`]); a refused name is
//! `INVALID_ARGUMENT` with the reason, never cut, so the agent can try again.

use tddy_coding_agent_specs::Capability;
use tddy_core::session_display_name::{rename_session, RenameSessionError};
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{RenameOwnSessionRequest, RenameOwnSessionResponse};

use super::operator_notifications::hook_token_matches;
use crate::service::ActivityServiceImpl;

impl ActivityServiceImpl {
    /// Authenticate the caller as the session's own agent, then record or clear its name.
    pub(super) async fn rename_own_session_impl(
        &self,
        req: RenameOwnSessionRequest,
    ) -> Result<Response<RenameOwnSessionResponse>, Status> {
        let (_, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;

        // `operator_notify` is the capability a launch exports the environment `tddy-tools` reads
        // under; refused before the credential, as the notification is, so a command run in a
        // session of the wrong type is told that rather than that its token is bad.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if !self.session_offers(session_type, Capability::OperatorNotify) {
            return Err(Status::failed_precondition(format!(
                "session_type {session_type:?} is not an agent session that declares operator_notify"
            )));
        }
        if !hook_token_matches(meta.hook_token.as_deref(), &req.hook_token) {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        let display_name = req.display_name;
        let recorded =
            tokio::task::spawn_blocking(move || rename_session(&session_dir, &display_name))
                .await
                .map_err(|e| Status::internal(format!("renaming the session failed: {e}")))?
                .map_err(|e| match e {
                    RenameSessionError::Invalid(e) => Status::invalid_argument(e.to_string()),
                    RenameSessionError::Unreadable(_) => Status::not_found("session not found"),
                    RenameSessionError::Unwritable(e) => {
                        Status::internal(format!("cannot record the session's name: {e}"))
                    }
                })?;
        Ok(Response::new(RenameOwnSessionResponse {
            display_name: recorded.unwrap_or_default(),
        }))
    }
}
