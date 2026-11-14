//! `SetProjectOrchestratorDefaults` (`#orchestrators` 11/11): what a new orchestrator for a project
//! starts with on one host — the "+" composer's model, effort and provider account, saved from the
//! project's settings pane.
//!
//! **Per-host scope**, as main is: the request is routed to the host its `daemon_instance_id`
//! names ([`ProjectRpcHandler::coordinate_route`]) and written to that host's `projects.yaml` row
//! only — never fanned out to the project's other hosts.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    SetProjectOrchestratorDefaultsRequest, SetProjectOrchestratorDefaultsResponse,
};

use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    /// Save (or clear, when `defaults` is absent) the project's orchestrator defaults on the host
    /// the request names, answering with that host's row. An unknown project is `not_found`; an
    /// empty `project_id` is `invalid_argument`; an unknown host is `failed_precondition`.
    pub(crate) async fn set_project_orchestrator_defaults_at_project_coordinate(
        &self,
        request: Request<SetProjectOrchestratorDefaultsRequest>,
    ) -> Result<Response<SetProjectOrchestratorDefaultsResponse>, Status> {
        // TODO(project-settings-pane): implement
        let _ = request.into_inner();
        Err(Status::unimplemented(
            "SetProjectOrchestratorDefaults is not implemented yet",
        ))
    }
}
