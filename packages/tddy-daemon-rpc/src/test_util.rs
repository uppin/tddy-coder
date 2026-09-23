//! Shared test helpers for the families this crate serves, on top of
//! [`tddy_session_lifecycle::test_util`].
//!
//! Import with:
//! ```ignore
//! use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
//! ```

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::catalog::{
    CatalogService, ListAgentModelsRequest, ListAgentModelsResponse, ListAgentsRequest,
    ListAgentsResponse, ListSubagentsRequest, ListSubagentsResponse, ListToolsRequest,
    ListToolsResponse,
};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ProjectService, SetProjectDefaultBranchRequest,
    SetProjectDefaultBranchResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

use crate::RpcHandlers;

/// A lifecycle [`TestDaemon`](tddy_session_lifecycle::test_util::TestDaemon) with this crate's
/// [`RpcHandlers`] installed as its `DaemonRpcFamilies` — what `runtime::build` assembles — and
/// answering the families served here through those same handlers.
#[derive(Clone)]
pub struct TestDaemon {
    daemon: tddy_session_lifecycle::test_util::TestDaemon,
    handlers: RpcHandlers,
}

impl TestDaemon {
    /// Install the handlers on `host`, which must already carry every `with_*` the suite wants:
    /// the handlers share its state from here on (see [`RpcHandlers::install`]).
    #[must_use]
    pub fn from_host(host: DaemonSessionHost) -> Self {
        let (host, handlers) = RpcHandlers::install(host);
        Self {
            daemon: tddy_session_lifecycle::test_util::TestDaemon::from_arc(Arc::new(host)),
            handlers,
        }
    }

    /// The handlers this daemon serves, and installed on its host.
    #[must_use]
    pub fn handlers(&self) -> &RpcHandlers {
        &self.handlers
    }

    /// Same contract as the lifecycle `TestDaemon`'s. Safe after the handlers are installed: the
    /// roster cadence is the host's alone, and no handler holds it.
    #[must_use]
    pub fn with_roster_keepalive_interval(mut self, interval: std::time::Duration) -> Self {
        self.daemon = self.daemon.with_roster_keepalive_interval(interval);
        self
    }
}

/// [`tddy_session_lifecycle::test_util::test_service`], with this crate's [`RpcHandlers`]
/// installed: the same host, resolvers and sandbox RPC bridge, answering the families served here
/// through the handlers `runtime::build` would build.
///
/// [`TEST_TOKEN`](tddy_session_lifecycle::test_util::TEST_TOKEN) resolves to
/// [`TEST_USER`](tddy_session_lifecycle::test_util::TEST_USER); any other token returns `None`.
#[must_use]
pub fn test_service(sessions_base: PathBuf) -> TestDaemon {
    let daemon = TestDaemon::from_host(tddy_session_lifecycle::test_util::test_host(sessions_base));
    daemon.as_arc().install_sandbox_rpc_bridge();
    daemon
}

impl Deref for TestDaemon {
    type Target = tddy_session_lifecycle::test_util::TestDaemon;

    fn deref(&self) -> &Self::Target {
        &self.daemon
    }
}

#[async_trait]
impl ProjectService for TestDaemon {
    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        self.handlers.project_service().list_projects(request).await
    }

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        self.handlers
            .project_service()
            .create_project(request)
            .await
    }

    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        self.handlers
            .project_service()
            .add_project_to_host(request)
            .await
    }

    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        self.handlers
            .project_service()
            .list_project_branches(request)
            .await
    }

    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
        self.handlers
            .project_service()
            .set_project_default_branch(request)
            .await
    }
}

#[async_trait]
impl CatalogService for TestDaemon {
    async fn list_tools(
        &self,
        request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.handlers.catalog_service().list_tools(request).await
    }

    async fn list_agents(
        &self,
        request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        self.handlers.catalog_service().list_agents(request).await
    }

    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        self.handlers
            .catalog_service()
            .list_agent_models(request)
            .await
    }

    async fn list_subagents(
        &self,
        request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        self.handlers
            .catalog_service()
            .list_subagents(request)
            .await
    }
}
