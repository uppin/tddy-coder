//! Port the six `project.ProjectService` RPCs are implemented against.

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, SetProjectAccountsRequest, SetProjectAccountsResponse,
    SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
};

/// What the six project RPCs need from the host process.
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

    /// Replace which account the project uses at each provider. Logical-project scope: the host
    /// forwards it to peers owning the same `project_id`, as it does the default branch.
    async fn set_project_accounts(
        &self,
        request: Request<SetProjectAccountsRequest>,
    ) -> Result<Response<SetProjectAccountsResponse>, Status>;
}
