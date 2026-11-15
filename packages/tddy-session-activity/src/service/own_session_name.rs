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

use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{RenameOwnSessionRequest, RenameOwnSessionResponse};

use crate::service::ActivityServiceImpl;

impl ActivityServiceImpl {
    /// Authenticate the caller as the session's own agent, then record or clear its name.
    pub(super) async fn rename_own_session_impl(
        &self,
        req: RenameOwnSessionRequest,
    ) -> Result<Response<RenameOwnSessionResponse>, Status> {
        // TODO(session-display-name): green phase.
        let _ = req;
        Err(Status::unimplemented("RenameOwnSession"))
    }
}
