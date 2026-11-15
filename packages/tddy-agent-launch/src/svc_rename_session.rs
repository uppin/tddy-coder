//! `RenameSession`: record or clear the name of a session this daemon holds.
//!
//! The name lives in the session's `.session.yaml` (`tddy_core::session_display_name`), so the
//! daemon holding the session answers; a client sends the call to the session's owning host, as it
//! does `DeleteSession`.

use tddy_core::session_display_name::{rename_session, RenameSessionError};
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{RenameSessionRequest, RenameSessionResponse};

use crate::launch_ports::LaunchSessions;

impl LaunchSessions {
    pub async fn rename_session_at_session_coordinate(
        &self,
        request: Request<RenameSessionRequest>,
    ) -> Result<Response<RenameSessionResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.os_user_of_session_token(&req.session_token)?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let recorded =
            tokio::task::spawn_blocking(move || rename_session(&session_dir, &req.display_name))
                .await
                .map_err(|e| Status::internal(format!("renaming the session failed: {e}")))?
                .map_err(status_of_refused_rename)?;
        Ok(Response::new(RenameSessionResponse {
            display_name: recorded.unwrap_or_default(),
        }))
    }
}

/// The status a refused rename answers with: the name's own fault, a session this daemon does not
/// hold, or a record it could not write.
pub(crate) fn status_of_refused_rename(error: RenameSessionError) -> Status {
    match error {
        RenameSessionError::Invalid(e) => Status::invalid_argument(e.to_string()),
        RenameSessionError::Unreadable(_) => Status::not_found("session not found"),
        RenameSessionError::Unwritable(e) => {
            Status::internal(format!("cannot record the session's name: {e}"))
        }
    }
}
