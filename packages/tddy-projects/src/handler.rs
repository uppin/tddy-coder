//! Port the ten `project.ProjectService` RPCs are implemented against.

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, SetProjectAccountsRequest, SetProjectAccountsResponse,
    SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
    SetProjectOrchestratorDefaultsRequest, SetProjectOrchestratorDefaultsResponse,
};
use tddy_service::proto::project::{
    DeleteProjectRequest, DeleteProjectResponse, RestoreProjectRequest, RestoreProjectResponse,
    UpdateProjectRequest, UpdateProjectResponse,
};

/// What the ten project RPCs need from the host process.
#[async_trait]
pub trait ProjectHandler: Send + Sync {
    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status>;

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status>;

    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status>;

    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status>;

    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status>;

    /// Replace which account the project uses at each provider, on the one host the request names
    /// (see `docs/dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md`).
    async fn set_project_accounts(
        &self,
        request: Request<SetProjectAccountsRequest>,
    ) -> Result<Response<SetProjectAccountsResponse>, Status>;

    /// Save (or clear) what a new orchestrator for the project starts with on one host. Per-host
    /// scope: routed to the host the request names, never fanned out (`#orchestrators` 11/11).
    async fn set_project_orchestrator_defaults(
        &self,
        request: Request<SetProjectOrchestratorDefaultsRequest>,
    ) -> Result<Response<SetProjectOrchestratorDefaultsResponse>, Status>;

    /// Rename the project on every host owning it: written here, then fanned out to every eligible
    /// peer unless the request is `local_only`.
    async fn update_project(
        &self,
        request: Request<UpdateProjectRequest>,
    ) -> Result<Response<UpdateProjectResponse>, Status>;

    /// Soft-delete the project on every host owning it, with the same fan-out. Nothing is removed
    /// from disk.
    async fn delete_project(
        &self,
        request: Request<DeleteProjectRequest>,
    ) -> Result<Response<DeleteProjectResponse>, Status>;

    /// Clear the project's tombstone on every host owning it, with the same fan-out.
    async fn restore_project(
        &self,
        request: Request<RestoreProjectRequest>,
    ) -> Result<Response<RestoreProjectResponse>, Status>;
}
