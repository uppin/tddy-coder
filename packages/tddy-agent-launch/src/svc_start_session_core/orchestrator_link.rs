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
