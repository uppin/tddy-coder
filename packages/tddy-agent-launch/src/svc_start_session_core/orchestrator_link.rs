use crate::launch_ports::LaunchSessions;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_service::proto::session::StartSessionRequest;
use tddy_session_split::service_util::OrchestratorLink;

impl LaunchSessions {
    /// The orchestrator `req` names, as the first changeset of the session it starts records it:
    /// written before the session's agent is spawned, so who may answer its prompts is on record
    /// from the start. An orchestrator on this daemon carries no host; one on another daemon names
    /// it. `None` when `req` names no orchestrator.
    pub(crate) fn orchestrator_link_of(
        &self,
        req: &StartSessionRequest,
    ) -> Option<OrchestratorLink> {
        OrchestratorLink::of_request(
            &req.orchestrator_session_id,
            &req.orchestrator_daemon_instance_id,
            &local_instance_id_for_config(&self.config),
        )
    }
}

impl LaunchSessions {
    /// What the session in `session_dir` reports to on a resume, as its start did: the orchestrator
    /// its changeset records ([`Self::resumed_orchestrator_link`]), through [`Self::reports_to_of`].
    pub(crate) fn resumed_reports_to(
        &self,
        session_dir: &std::path::Path,
    ) -> Option<crate::back_channel::ReportsTo<'_>> {
        let sessions_base = session_dir.parent().and_then(std::path::Path::parent)?;
        self.reports_to_of(
            self.resumed_orchestrator_link(session_dir).as_ref(),
            sessions_base,
        )
    }

    /// The orchestrator a session's changeset records, when it runs on this daemon: what a resume
    /// of the session reports to, as its start did.
    fn resumed_orchestrator_link(&self, session_dir: &std::path::Path) -> Option<OrchestratorLink> {
        let changeset = tddy_core::read_changeset(session_dir).ok()?;
        OrchestratorLink::of_request(
            changeset
                .orchestrator_session_id
                .as_deref()
                .unwrap_or_default(),
            changeset
                .orchestrator_daemon_instance_id
                .as_deref()
                .unwrap_or_default(),
            &local_instance_id_for_config(&self.config),
        )
    }

    /// What a session started by `orchestrator` reports to (`#orchestra` 2/9): the orchestrator's
    /// session id and, when this host recorded it as a coding agent's session, that agent. `None`
    /// when the start names no orchestrator, or one on another daemon — a cross-host report is not
    /// carried here.
    pub(crate) fn reports_to_of(
        &self,
        orchestrator: Option<&OrchestratorLink>,
        sessions_base: &std::path::Path,
    ) -> Option<crate::back_channel::ReportsTo<'_>> {
        let orchestrator = orchestrator.filter(|link| link.daemon_instance_id.is_none())?;
        let session_type = tddy_core::read_session_metadata(
            &tddy_core::session_lifecycle::unified_session_dir_path(
                sessions_base,
                &orchestrator.session_id,
            ),
        )
        .ok()
        .and_then(|metadata| metadata.session_type);
        Some(crate::back_channel::ReportsTo {
            orchestrator_session_id: orchestrator.session_id.clone(),
            orchestrator_agent: session_type.and_then(|session_type| {
                self.coding_agents.agents().for_session_type(&session_type)
            }),
        })
    }
}
