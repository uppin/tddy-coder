//! Family D — host side of [`tddy_projects::ProjectHandler`].

use async_trait::async_trait;
use tddy_projects::ProjectHandler;
use tddy_rpc::{Request, Response, Status};

use super::ConnectionServiceImpl;

#[async_trait]
impl ProjectHandler for ConnectionServiceImpl {
    async fn list_projects(
        &self,
        request: Request<tddy_service::proto::project::ListProjectsRequest>,
    ) -> Result<Response<tddy_service::proto::project::ListProjectsResponse>, Status> {
        self.list_projects_at_project_coordinate(request).await
    }

    async fn create_project(
        &self,
        request: Request<tddy_service::proto::project::CreateProjectRequest>,
    ) -> Result<Response<tddy_service::proto::project::CreateProjectResponse>, Status> {
        self.create_project_at_project_coordinate(request).await
    }

    async fn add_project_to_host(
        &self,
        request: Request<tddy_service::proto::project::AddProjectToHostRequest>,
    ) -> Result<Response<tddy_service::proto::project::AddProjectToHostResponse>, Status> {
        self.add_project_to_host_at_project_coordinate(request).await
    }

    async fn list_project_branches(
        &self,
        request: Request<tddy_service::proto::project::ListProjectBranchesRequest>,
    ) -> Result<Response<tddy_service::proto::project::ListProjectBranchesResponse>, Status> {
        self.list_project_branches_at_project_coordinate(request).await
    }

    async fn set_project_default_branch(
        &self,
        request: Request<tddy_service::proto::project::SetProjectDefaultBranchRequest>,
    ) -> Result<Response<tddy_service::proto::project::SetProjectDefaultBranchResponse>, Status> {
        self.set_project_default_branch_at_project_coordinate(request).await
    }
}

impl ConnectionServiceImpl {
    #[must_use]
    pub fn project_service(
        self: &std::sync::Arc<Self>,
    ) -> tddy_projects::ProjectServiceImpl<Self> {
        tddy_projects::ProjectServiceImpl::new(std::sync::Arc::clone(self))
    }

    #[must_use]
    pub fn project_entry(self: &std::sync::Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_projects::build_project_entry(self.project_service())
    }
}
