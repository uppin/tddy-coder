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
use tddy_host_service::HostServiceImpl;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::catalog::{
    CatalogService, ListAgentModelsRequest, ListAgentModelsResponse, ListAgentsRequest,
    ListAgentsResponse, ListSubagentsRequest, ListSubagentsResponse, ListToolsRequest,
    ListToolsResponse,
};
use tddy_service::proto::exec_tools::{
    ApprovalRequest, ApprovalResponse, ConversationWorktreeRequest, ConversationWorktreeResponse,
    ExecToolService, ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse,
    ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse,
};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ProjectService, SetProjectAccountsRequest, SetProjectAccountsResponse,
    SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
    SetProjectOrchestratorDefaultsRequest, SetProjectOrchestratorDefaultsResponse,
};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetWorkQueueRequest, GetWorkQueueResponse, LinkWorkItemRequest, LinkWorkItemResponse,
    ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse, WorkQueueService,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_worktree_service::stream::MpscResultStream;

use crate::RpcHandlers;

/// The host service a test daemon's project-home tools answer from when a suite attaches
/// none of its own: one built from the host's own config and resolver, as the handler
/// itself was built before the service became the composition root's to attach. A suite
/// that needs fixture operator files or Coder accounts attaches its own to the handlers
/// it installs instead, and wraps them with [`TestDaemon::from_handlers`]. Built from the
/// host's parts, never the host itself — no handler here holds the session host
/// (`rpc_handlers_shape` AC4/AC5).
fn a_bare_host_service(
    config: &tddy_daemon_kernel::config::DaemonConfig,
    tddy_data_dir: &std::path::Path,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> Arc<HostServiceImpl> {
    Arc::new(HostServiceImpl::new(
        config.clone(),
        tddy_data_dir,
        user_resolver,
    ))
}

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
    /// the handlers share its state from here on (see [`RpcHandlers::install`]), and the bare
    /// host service is attached for them.
    #[must_use]
    pub fn from_host(host: DaemonSessionHost) -> Self {
        let host_service =
            a_bare_host_service(host.config(), host.tddy_data_dir(), host.user_resolver());
        let (host, handlers) = RpcHandlers::install(host);
        handlers.attach_host_service(host_service);
        Self::from_handlers(
            tddy_session_lifecycle::test_util::TestDaemon::from_arc(Arc::new(host)),
            handlers,
        )
    }

    /// Wrap handlers a suite installed on its own host and wired itself — above all the host
    /// service it attached through [`RpcHandlers::attach_host_service`], its fixture service
    /// over an operator home. No bare service is attached here: the handlers arrive complete.
    #[must_use]
    pub fn from_handlers(
        daemon: tddy_session_lifecycle::test_util::TestDaemon,
        handlers: RpcHandlers,
    ) -> Self {
        Self { daemon, handlers }
    }

    /// Serve the families here for a lifecycle `daemon` whose host is already behind its `Arc` —
    /// one a suite built with [`RpcHandlers::install`] and also serves over LiveKit. The handlers
    /// answering here are built from the same host, so they share every `Arc` with the ones
    /// installed on it: the same task registry, jails, roster and idle tracker, not copies.
    #[must_use]
    pub fn serving(daemon: tddy_session_lifecycle::test_util::TestDaemon) -> Self {
        let handlers = RpcHandlers::from_host(daemon.connection());
        let host = daemon.connection();
        handlers.attach_host_service(a_bare_host_service(
            host.config(),
            host.tddy_data_dir(),
            host.user_resolver(),
        ));
        Self { daemon, handlers }
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

    /// Same contract as the lifecycle `TestDaemon`'s. Safe after the handlers are installed: what
    /// builds a jail is the host's alone, and the exec-tool handler holds only the registry the
    /// jails it builds land in — the same one the host holds.
    #[must_use]
    pub fn with_workspace_sandbox_provisioner(
        mut self,
        provisioner: Arc<
            dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner,
        >,
    ) -> Self {
        self.daemon = self.daemon.with_workspace_sandbox_provisioner(provisioner);
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

/// [`test_service`] with `launcher` starting every coding-agent subagent — a test injects one that
/// records each launch and points it at `tddy-acp-stub`.
pub fn test_service_with_coding_agent_launcher(
    sessions_base: PathBuf,
    launcher: Arc<dyn tddy_coding_subagent::CodingAgentLauncher>,
) -> TestDaemon {
    let daemon = TestDaemon::from_host(
        tddy_session_lifecycle::test_util::test_host(sessions_base)
            .with_coding_agent_launcher(launcher),
    );
    daemon.as_arc().install_sandbox_rpc_bridge();
    daemon
}

/// [`test_service_with_coding_agent_launcher`] over a host that authenticates real session tokens
/// ([`tddy_session_lifecycle::test_util::signing_test_host`]), with the credentials it accepts.
pub fn signing_test_service_with_coding_agent_launcher(
    sessions_base: PathBuf,
    launcher: Arc<dyn tddy_coding_subagent::CodingAgentLauncher>,
) -> (
    TestDaemon,
    tddy_session_lifecycle::test_util::SignedTestCredentials,
) {
    let (host, credentials) = tddy_session_lifecycle::test_util::signing_test_host(sessions_base);
    let daemon = TestDaemon::from_host(host.with_coding_agent_launcher(launcher));
    daemon.as_arc().install_sandbox_rpc_bridge();
    (daemon, credentials)
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

    async fn set_project_accounts(
        &self,
        request: Request<SetProjectAccountsRequest>,
    ) -> Result<Response<SetProjectAccountsResponse>, Status> {
        self.handlers
            .project_service()
            .set_project_accounts(request)
            .await
    }

    async fn set_project_orchestrator_defaults(
        &self,
        request: Request<SetProjectOrchestratorDefaultsRequest>,
    ) -> Result<Response<SetProjectOrchestratorDefaultsResponse>, Status> {
        self.handlers
            .project_service()
            .set_project_orchestrator_defaults(request)
            .await
    }

    async fn update_project(
        &self,
        request: Request<tddy_service::proto::project::UpdateProjectRequest>,
    ) -> Result<Response<tddy_service::proto::project::UpdateProjectResponse>, Status> {
        self.handlers
            .project_service()
            .update_project(request)
            .await
    }

    async fn delete_project(
        &self,
        request: Request<tddy_service::proto::project::DeleteProjectRequest>,
    ) -> Result<Response<tddy_service::proto::project::DeleteProjectResponse>, Status> {
        self.handlers
            .project_service()
            .delete_project(request)
            .await
    }

    async fn restore_project(
        &self,
        request: Request<tddy_service::proto::project::RestoreProjectRequest>,
    ) -> Result<Response<tddy_service::proto::project::RestoreProjectResponse>, Status> {
        self.handlers
            .project_service()
            .restore_project(request)
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

#[async_trait]
impl ExecToolService for TestDaemon {
    type StreamExecuteToolStream = MpscResultStream<ExecuteToolChunk>;

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.handlers
            .exec_tool_service()
            .execute_tool(request)
            .await
    }

    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        self.handlers
            .exec_tool_service()
            .stream_execute_tool(request)
            .await
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        self.handlers
            .exec_tool_service()
            .list_exec_tools(request)
            .await
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.handlers
            .exec_tool_service()
            .list_session_tool_calls(request)
            .await
    }

    async fn conversation_worktree(
        &self,
        request: Request<ConversationWorktreeRequest>,
    ) -> Result<Response<ConversationWorktreeResponse>, Status> {
        self.handlers
            .exec_tool_service()
            .conversation_worktree(request)
            .await
    }

    async fn request_approval(
        &self,
        request: Request<ApprovalRequest>,
    ) -> Result<Response<ApprovalResponse>, Status> {
        self.handlers
            .exec_tool_service()
            .request_approval(request)
            .await
    }
}

#[async_trait]
impl WorkQueueService for TestDaemon {
    async fn add_work_item(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .add_work_item(request)
            .await
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_pr_status(request)
            .await
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        self.handlers
            .work_queue_service()
            .query_branch(request)
            .await
    }

    async fn resolve_work_item_base(
        &self,
        request: Request<ResolveWorkItemBaseRequest>,
    ) -> Result<Response<ResolveWorkItemBaseResponse>, Status> {
        self.handlers
            .work_queue_service()
            .resolve_work_item_base(request)
            .await
    }

    async fn link_work_item(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .link_work_item(request)
            .await
    }

    async fn repoint_work_item(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .repoint_work_item(request)
            .await
    }

    async fn reorder_work_item(
        &self,
        request: Request<ReorderWorkItemRequest>,
    ) -> Result<Response<ReorderWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .reorder_work_item(request)
            .await
    }

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        self.handlers
            .work_queue_service()
            .pull_base_into_branch(request)
            .await
    }

    async fn list_project_work_queues(
        &self,
        request: Request<ListProjectWorkQueuesRequest>,
    ) -> Result<Response<ListProjectWorkQueuesResponse>, Status> {
        self.handlers
            .work_queue_service()
            .list_project_work_queues(request)
            .await
    }

    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_work_queue(request)
            .await
    }

    type StreamWorkQueueStatusStream = tddy_work_queue::rpc::WorkQueueStatusStream;

    async fn stream_work_queue_status(
        &self,
        request: Request<tddy_service::proto::work_queue::StreamWorkQueueStatusRequest>,
    ) -> Result<Response<Self::StreamWorkQueueStatusStream>, Status> {
        self.handlers
            .work_queue_service()
            .stream_work_queue_status(request)
            .await
    }

    async fn assign_work_item_pr(
        &self,
        request: Request<tddy_service::proto::work_queue::AssignWorkItemPrRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .assign_work_item_pr(request)
            .await
    }

    async fn unassign_work_item_pr(
        &self,
        request: Request<tddy_service::proto::work_queue::UnassignWorkItemPrRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .unassign_work_item_pr(request)
            .await
    }

    async fn link_work_item_session(
        &self,
        request: Request<tddy_service::proto::work_queue::LinkWorkItemSessionRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .link_work_item_session(request)
            .await
    }

    async fn unlink_work_item_session(
        &self,
        request: Request<tddy_service::proto::work_queue::UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .unlink_work_item_session(request)
            .await
    }

    async fn set_work_item_labels(
        &self,
        request: Request<tddy_service::proto::work_queue::SetWorkItemLabelsRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .set_work_item_labels(request)
            .await
    }

    async fn set_work_item_badge(
        &self,
        request: Request<tddy_service::proto::work_queue::SetWorkItemBadgeRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .set_work_item_badge(request)
            .await
    }

    async fn clear_work_item_badge(
        &self,
        request: Request<tddy_service::proto::work_queue::ClearWorkItemBadgeRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .clear_work_item_badge(request)
            .await
    }

    async fn set_work_queue_dependencies(
        &self,
        request: Request<tddy_service::proto::work_queue::SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        self.handlers
            .work_queue_service()
            .set_work_queue_dependencies(request)
            .await
    }

    async fn get_work_queue_graph(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueGraphResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_work_queue_graph(request)
            .await
    }

    async fn get_work_queue_waves(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueWavesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueWavesResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_work_queue_waves(request)
            .await
    }
}
