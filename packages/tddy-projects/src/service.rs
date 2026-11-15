//! Thin `project.ProjectService` adapter over a [`ProjectHandler`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ProjectService, SetProjectAccountsRequest, SetProjectAccountsResponse,
    SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
    SetProjectOrchestratorDefaultsRequest, SetProjectOrchestratorDefaultsResponse,
};

use tddy_service::proto::project::{
    DeleteProjectRequest, DeleteProjectResponse, RestoreProjectRequest, RestoreProjectResponse,
    UpdateProjectRequest, UpdateProjectResponse,
};

use crate::handler::ProjectHandler;

/// Thin `ProjectService` adapter over a [`ProjectHandler`].
pub struct ProjectServiceImpl<H> {
    host: Arc<H>,
}

impl<H> ProjectServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: ProjectHandler + 'static> ProjectService for ProjectServiceImpl<H> {
    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        self.host.list_projects(request).await
    }

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        self.host.create_project(request).await
    }

    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        self.host.add_project_to_host(request).await
    }

    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        self.host.list_project_branches(request).await
    }

    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
        self.host.set_project_default_branch(request).await
    }

    async fn set_project_accounts(
        &self,
        request: Request<SetProjectAccountsRequest>,
    ) -> Result<Response<SetProjectAccountsResponse>, Status> {
        self.host.set_project_accounts(request).await
    }

    async fn set_project_orchestrator_defaults(
        &self,
        request: Request<SetProjectOrchestratorDefaultsRequest>,
    ) -> Result<Response<SetProjectOrchestratorDefaultsResponse>, Status> {
        self.host.set_project_orchestrator_defaults(request).await
    }

    async fn update_project(
        &self,
        request: Request<UpdateProjectRequest>,
    ) -> Result<Response<UpdateProjectResponse>, Status> {
        self.host.update_project(request).await
    }

    async fn delete_project(
        &self,
        request: Request<DeleteProjectRequest>,
    ) -> Result<Response<DeleteProjectResponse>, Status> {
        self.host.delete_project(request).await
    }

    async fn restore_project(
        &self,
        request: Request<RestoreProjectRequest>,
    ) -> Result<Response<RestoreProjectResponse>, Status> {
        self.host.restore_project(request).await
    }
}

/// Registers `project.ProjectService` on the RPC transport.
#[must_use]
pub fn build_project_entry<H>(service: ProjectServiceImpl<H>) -> tddy_rpc::ServiceEntry
where
    H: ProjectHandler + 'static,
{
    use tddy_service::proto::project::ProjectServiceServer;

    tddy_rpc::ServiceEntry {
        name: "project.ProjectService",
        service: Arc::new(ProjectServiceServer::new(service)) as Arc<dyn tddy_rpc::RpcService>,
    }
}
