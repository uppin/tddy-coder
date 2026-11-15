//! A `CreateProject` or `AddProjectToHost` that meets a soft-deleted project restores it on every
//! host owning it ([`ProjectRpcHandler::restore_everywhere`]) instead of adding a second row over
//! the checkout the project kept.

use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Response, Status};
use tddy_service::proto::project::{AddProjectToHostResponse, CreateProjectResponse};

use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    /// Answer a `CreateProject` that met `deleted` by restoring it on every host owning it.
    pub(super) async fn restore_instead_of_creating(
        &self,
        session_token: &str,
        git_url: &str,
        deleted: ProjectData,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        log::info!(
            "create_project: {git_url} is deleted project {}; restoring it",
            deleted.project_id
        );
        let caller = self.project_caller(session_token)?;
        let restored = self
            .restore_everywhere(&caller, session_token, &deleted.project_id)
            .await?;
        Ok(Response::new(CreateProjectResponse {
            project: restored.project,
        }))
    }

    /// Answer an `AddProjectToHost` that met `project_id`'s tombstone on this host by restoring it
    /// everywhere, with this host's restored row.
    pub(super) async fn restore_added_project(
        &self,
        session_token: &str,
        project_id: &str,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        log::info!("AddProjectToHost: project_id={project_id} is deleted; restoring it");
        let caller = self.project_caller(session_token)?;
        self.restore_everywhere(&caller, session_token, project_id)
            .await?;
        let row = project_storage::find_project(&caller.projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("project vanished after restore"))?;
        Ok(Response::new(AddProjectToHostResponse {
            project: Some(self.local_entry(&caller, &row)),
        }))
    }
}

/// A deleted project on this host for `git_url` or for the checkout at `checkout`.
pub(super) fn deleted_project_at(
    projects_dir: &std::path::Path,
    git_url: &str,
    checkout: &str,
) -> Result<Option<ProjectData>, Status> {
    project_storage::find_deleted_project_matching(projects_dir, git_url, checkout)
        .map_err(|e| Status::internal(e.to_string()))
}

/// `destination` as a registered row records it: canonical when it exists.
pub(super) fn checkout_of(destination: &std::path::Path) -> String {
    destination
        .canonicalize()
        .unwrap_or_else(|_| destination.to_path_buf())
        .display()
        .to_string()
}
