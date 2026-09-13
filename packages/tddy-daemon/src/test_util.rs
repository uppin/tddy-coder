//! Shared test helpers for tddy-daemon integration and acceptance tests.
//!
//! Import with:
//! ```ignore
//! use tddy_daemon::test_util::{test_config, test_service, TEST_TOKEN, TEST_USER};
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
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse,
    ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse,
};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetPrStatusRequest, GetPrStatusResponse,
    LinkStackNodeRequest, LinkStackNodeResponse, PrStackService, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderPlannedPrRequest,
    ReorderPlannedPrResponse, RepointPlannedPrRequest, RepointPlannedPrResponse,
    ResolveStackBaseRequest, ResolveStackBaseResponse,
};
use tddy_worktree_service::stream::MpscResultStream;

use crate::cli_session_manager::CliSessionManager;
use crate::config::DaemonConfig;
use crate::connection_service::ConnectionServiceImpl;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

/// Token accepted by [`test_service`] as a valid session token.
pub const TEST_TOKEN: &str = "valid-token";
/// OS user returned for [`TEST_TOKEN`] by [`test_service`].
pub const TEST_USER: &str = "testuser";

const CONFIG_YAML: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;

/// Build a minimal [`DaemonConfig`] suitable for unit/acceptance tests.
pub fn test_config() -> DaemonConfig {
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, CONFIG_YAML).expect("write test config");
    DaemonConfig::load(&path).expect("load test config")
}

fn new_connection_service(sessions_base: PathBuf) -> Arc<ConnectionServiceImpl> {
    let config = test_config();
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
            Some(TEST_USER.to_string())
        } else {
            None
        }
    });
    let service = Arc::new(ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    ));
    install_self_handle(&service);
    service
}

/// Record the weak back-pointer [`ConnectionServiceImpl::self_arc`] needs — same wiring as
/// `runtime::build` right after its `Arc::new`.
pub fn install_self_handle(service: &Arc<ConnectionServiceImpl>) {
    service.set_self_handle(Arc::downgrade(service));
}

/// Daemon under test: the connection service plus the catalogue, exec-tool and PR-stack families
/// unbundled onto their own coordinates (`#unbundle` node 8).
#[derive(Clone)]
pub struct TestDaemon {
    inner: Arc<ConnectionServiceImpl>,
}

impl TestDaemon {
    #[must_use]
    pub fn from_arc(inner: Arc<ConnectionServiceImpl>) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn connection(&self) -> &ConnectionServiceImpl {
        self.inner.as_ref()
    }

    #[must_use]
    pub fn as_arc(&self) -> Arc<ConnectionServiceImpl> {
        Arc::clone(&self.inner)
    }

    /// Substitute what builds a sandboxed workspace session's jail — same contract as
    /// [`ConnectionServiceImpl::with_workspace_sandbox_provisioner`], but safe on the shared
    /// `Arc` tests keep inside a [`TestDaemon`].
    pub fn with_workspace_sandbox_provisioner(
        mut self,
        provisioner: Arc<
            dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner,
        >,
    ) -> Self {
        Arc::make_mut(&mut self.inner).set_workspace_sandbox_provisioner(provisioner);
        self
    }

    pub fn with_staging_base_dir(mut self, staging_base_dir: PathBuf) -> Self {
        Arc::make_mut(&mut self.inner).set_staging_base_dir(staging_base_dir);
        self
    }

    pub fn with_eligible_daemon_source(
        mut self,
        eligible_daemon_source: Arc<dyn crate::multi_host::EligibleDaemonSource>,
    ) -> Self {
        Arc::make_mut(&mut self.inner).set_eligible_daemon_source(eligible_daemon_source);
        self
    }

    pub fn with_roster_keepalive_interval(mut self, interval: std::time::Duration) -> Self {
        Arc::make_mut(&mut self.inner).set_roster_keepalive_interval(interval);
        self
    }
}

impl Deref for TestDaemon {
    type Target = ConnectionServiceImpl;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

#[async_trait]
impl CatalogService for TestDaemon {
    async fn list_tools(
        &self,
        request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.inner.catalog_rpc_service().list_tools(request).await
    }

    async fn list_agents(
        &self,
        request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        self.inner.catalog_rpc_service().list_agents(request).await
    }

    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        self.inner
            .catalog_rpc_service()
            .list_agent_models(request)
            .await
    }

    async fn list_subagents(
        &self,
        request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        self.inner
            .catalog_rpc_service()
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
        self.inner
            .exec_tool_rpc_service()
            .execute_tool(request)
            .await
    }

    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        self.inner
            .exec_tool_rpc_service()
            .stream_execute_tool(request)
            .await
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        self.inner
            .exec_tool_rpc_service()
            .list_exec_tools(request)
            .await
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.inner
            .exec_tool_rpc_service()
            .list_session_tool_calls(request)
            .await
    }
}

#[async_trait]
impl PrStackService for TestDaemon {
    async fn add_planned_pr(
        &self,
        request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .add_planned_pr(request)
            .await
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .get_pr_status(request)
            .await
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .query_branch(request)
            .await
    }

    async fn resolve_stack_base(
        &self,
        request: Request<ResolveStackBaseRequest>,
    ) -> Result<Response<ResolveStackBaseResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .resolve_stack_base(request)
            .await
    }

    async fn link_stack_node(
        &self,
        request: Request<LinkStackNodeRequest>,
    ) -> Result<Response<LinkStackNodeResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .link_stack_node(request)
            .await
    }

    async fn repoint_planned_pr(
        &self,
        request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .repoint_planned_pr(request)
            .await
    }

    async fn reorder_planned_pr(
        &self,
        request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .reorder_planned_pr(request)
            .await
    }

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        self.inner
            .pr_stack_rpc_service()
            .pull_base_into_branch(request)
            .await
    }
}

/// Build a [`TestDaemon`] wired to `sessions_base` with the standard test resolvers.
///
/// [`TEST_TOKEN`] resolves to [`TEST_USER`]; any other token returns `None`.
pub fn test_service(sessions_base: PathBuf) -> TestDaemon {
    TestDaemon {
        inner: new_connection_service(sessions_base),
    }
}

/// Block until `host.HostService` lists `peer_instance_id` among this daemon's eligible peers.
///
/// The wait is asked through the RPC, not through the roster behind it: what these suites need to
/// know before they route anything is that the *call* a client would make answers with the peer, and
/// a source that holds a row the handler would not report is exactly the failure worth catching.
///
/// The host service is built here from `service`'s own [`ConnectionServiceImpl::routing_view`], so
/// the roster this waits on is the one the connection service will classify the subsequent route
/// against. Two sources would let this return on a peer the route then cannot find.
///
/// Panics with the list that *was* returned rather than a bare timeout: "these three daemons were
/// visible and yours was not" is a different bug report from "nothing happened".
pub async fn wait_until_peer_discovered(
    service: &ConnectionServiceImpl,
    session_token: &str,
    peer_instance_id: &str,
    timeout: std::time::Duration,
) {
    use tddy_service::proto::host::HostService as _;

    // The service's *own* config, roster and token resolver — so a token this suite already uses
    // resolves here exactly as it does on the call being waited for. `tddy_data_dir` is irrelevant:
    // `ListEligibleDaemons` reads no filesystem, and a path that does not exist is a clearer
    // statement of that than a tempdir nothing writes to.
    let (config, eligible, user_resolver) = service.routing_view();
    let hosts = tddy_host_service::HostServiceImpl::new(
        config,
        std::path::Path::new("/nonexistent-list-eligible-daemons-reads-no-files"),
        user_resolver,
    )
    .with_eligible_daemon_source(eligible);

    let deadline = std::time::Instant::now() + timeout;
    loop {
        let daemons = hosts
            .list_eligible_daemons(tddy_rpc::Request::new(
                tddy_service::proto::host::ListEligibleDaemonsRequest {
                    session_token: session_token.to_string(),
                },
            ))
            .await
            .expect("ListEligibleDaemons")
            .into_inner()
            .daemons;
        if daemons.iter().any(|d| d.instance_id == peer_instance_id) {
            return;
        }
        let visible: Vec<String> = daemons.into_iter().map(|d| d.instance_id).collect();
        assert!(
            std::time::Instant::now() < deadline,
            "daemon {peer_instance_id} never appeared in ListEligibleDaemons within {timeout:?}; \
             visible instead: {visible:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}

/// Join `ws_url` with `token`, serving every coordinate a daemon's RPC participant answers a
/// *forwarded* call on, and run it until the returned handle is dropped or aborted.
///
/// One helper rather than one per suite. Which coordinates a peer serves is a fact about the
/// daemon, not about the suite that pins a forward, and a peer that does not mount a coordinate
/// answers a forward to it with `Unknown service` — so four copies of this list is how three
/// cross-host suites came to still be serving `connection.ConnectionService` alone after
/// `#unbundle` node 6 moved the thirteen session-file RPCs onto
/// `session_files.SessionFilesService`.
///
/// The production roster is `runtime::build`'s, which mounts these two among several more; the
/// extras are the ones no forward in these suites addresses, and each needs wiring a test daemon
/// does not have.
pub async fn serve_daemon_rpc_participant(
    ws_url: &str,
    token: &str,
    service: &Arc<ConnectionServiceImpl>,
) -> tokio::task::JoinHandle<()> {
    let roster = tddy_rpc::MultiRpcService::new(vec![
        service.session_files_entry(),
        service.session_agents_entry(),
        service.activity_entry(),
        service.catalog_entry(),
        service.exec_tool_entry(),
        service.pr_stack_entry(),
        tddy_rpc::ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(tddy_service::ConnectionServiceServer::from_arc(Arc::clone(
                service,
            ))) as Arc<dyn tddy_rpc::RpcService>,
        },
    ]);
    let participant = tddy_livekit::LiveKitParticipant::connect(
        ws_url,
        token,
        roster,
        Default::default(),
        None,
        None,
    )
    .await
    .expect("daemon joins the room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}
