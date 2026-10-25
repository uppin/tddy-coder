use crate::launch_ports::LaunchSessions;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionResponse;

impl LaunchSessions {
    /// Record `orchestrator` — on `orchestrator_host`, when that is another daemon — in the changeset
    /// of the session `started` names. A start that created nothing (a reported branch conflict)
    /// records nothing.
    ///
    /// TODO(start-parity): written once the start has returned, after the session's agent is
    /// running, rather than in the changeset each session type first writes
    /// (`write_initial_changeset`, the workspace start). Threading it there removes the
    /// read-modify-write.
    pub(super) fn record_orchestrator(
        &self,
        os_user: &str,
        started: &StartSessionResponse,
        orchestrator: &str,
        orchestrator_host: Option<String>,
    ) -> Result<(), Status> {
        if started.session_id.is_empty() {
            return Ok(());
        }
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &started.session_id);
        let recorded = tddy_core::read_changeset(&session_dir).and_then(|mut changeset| {
            changeset.orchestrator_session_id = Some(orchestrator.to_string());
            changeset.orchestrator_daemon_instance_id = orchestrator_host;
            tddy_core::write_changeset(&session_dir, &changeset)
        });
        recorded.map_err(|e| {
            Status::internal(format!(
                "session {} was started, but its orchestrator {orchestrator} could not be recorded \
                 in its changeset: {e}",
                started.session_id
            ))
        })?;
        log::info!(
            "StartSession: session {} recorded with orchestrator {orchestrator}",
            started.session_id
        );
        Ok(())
    }
}
