//! `RenameSession`: record or clear the name of a session this daemon holds.
//!
//! The name lives in the session's `.session.yaml` (`tddy_core::session_display_name`), so the
//! daemon holding the session answers; a client sends the call to the session's owning host, as it
//! does `DeleteSession`.

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{RenameSessionRequest, RenameSessionResponse};

use crate::launch_ports::LaunchSessions;

impl LaunchSessions {
    pub async fn rename_session_at_session_coordinate(
        &self,
        request: Request<RenameSessionRequest>,
    ) -> Result<Response<RenameSessionResponse>, Status> {
        // TODO(session-display-name): green phase.
        let _ = request;
        Err(Status::unimplemented("RenameSession"))
    }
}
