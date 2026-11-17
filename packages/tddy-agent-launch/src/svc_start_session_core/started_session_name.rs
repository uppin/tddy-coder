use crate::launch_ports::LaunchSessions;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Response;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionResponse;

impl LaunchSessions {
    /// Record `display_name` in the `.session.yaml` of the session `started` names, once the start
    /// has created it here. One place for every session type this daemon writes the record of — a
    /// forwarded start is named by the peer that creates it, and a start refused for an owned branch
    /// created nothing. A record that cannot be written fails the start's answer rather than leaving
    /// a session silently unnamed.
    pub(super) async fn record_the_started_sessions_name(
        &self,
        os_user: &str,
        started: &Response<StartSessionResponse>,
        display_name: Option<String>,
    ) -> Result<(), Status> {
        let session_id = started.get_ref().session_id.clone();
        let Some(display_name) = display_name.filter(|_| !session_id.is_empty()) else {
            return Ok(());
        };
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &session_id);
        tokio::task::spawn_blocking(move || {
            tddy_core::session_display_name::rename_session(&session_dir, &display_name)
        })
        .await
        .map_err(|e| Status::internal(format!("naming the started session failed: {e}")))?
        .map_err(|e| {
            Status::internal(format!(
                "session {session_id} started, but its name could not be recorded: {e}"
            ))
        })?;
        Ok(())
    }
}
