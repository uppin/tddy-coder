//! `SetProjectOrchestratorDefaults` (`#orchestrators` 11/11): what a new orchestrator for a project
//! starts with on one host — the "+" composer's model, effort and provider account, saved from the
//! project's settings pane.
//!
//! **Per-host scope**, as main is: the request is routed to the host its `daemon_instance_id`
//! names ([`ProjectRpcHandler::coordinate_route`]) and written to that host's `projects.yaml` row
//! only — never fanned out to the project's other hosts.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md

use std::path::PathBuf;

use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_projects::project_storage::{self, OrchestratorDefaults as StoredDefaults};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    OrchestratorDefaults, SetProjectOrchestratorDefaultsRequest,
    SetProjectOrchestratorDefaultsResponse,
};
use tddy_session_lifecycle::connection_service::wire_same;

use super::coordinate_route::CoordinateRoute;
use super::entries;
use super::ProjectRpcHandler;

fn stored_from(proto: &OrchestratorDefaults) -> StoredDefaults {
    let some = |v: &str| (!v.trim().is_empty()).then(|| v.to_string());
    StoredDefaults {
        model: some(&proto.model),
        effort: some(&proto.effort),
        provider_account_id: some(&proto.provider_account_id),
    }
}

impl ProjectRpcHandler {
    /// Save (or clear, when `defaults` is absent) the project's orchestrator defaults on the host
    /// the request names, answering with that host's row. An unknown project is `not_found`; an
    /// empty `project_id` is `invalid_argument`; an unknown host is `failed_precondition`.
    pub(crate) async fn set_project_orchestrator_defaults_at_project_coordinate(
        &self,
        request: Request<SetProjectOrchestratorDefaultsRequest>,
    ) -> Result<Response<SetProjectOrchestratorDefaultsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let local_id = match self
            .coordinate_route("SetProjectOrchestratorDefaults", &req.daemon_instance_id)?
        {
            CoordinateRoute::Local { local_id } => local_id,
            CoordinateRoute::Forward {
                room,
                peer_instance_id,
            } => {
                let inner =
                    tddy_daemon_kernel::peer_calls::forward_set_project_orchestrator_defaults(
                        room,
                        &peer_instance_id,
                        &wire_same(&req)?,
                    )
                    .await?;
                return Ok(Response::new(wire_same(&inner)?));
            }
        };
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        if project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("project not found"));
        }
        let stored = req.defaults.as_ref().map(stored_from);
        project_storage::set_project_orchestrator_defaults(
            &projects_dir,
            project_id,
            stored.as_ref(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let row = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("project vanished after write"))?;
        let repo_root = PathBuf::from(&row.main_repo_path);
        let default_remote =
            entries::resolve_default_remote_or_empty(&projects_dir, &row.project_id, &repo_root);
        Ok(Response::new(SetProjectOrchestratorDefaultsResponse {
            project: Some(wire_same(&entries::project_entry_from(
                &row,
                local_id,
                default_remote,
            ))?),
        }))
    }
}
