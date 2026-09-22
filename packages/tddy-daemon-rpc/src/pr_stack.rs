//! Family P — `pr_stack.PrStackService`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use livekit::prelude::Room;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_github::GitHubTokenStore;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetPrStatusRequest, GetPrStatusResponse,
    LinkStackNodeRequest, LinkStackNodeResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderPlannedPrRequest,
    ReorderPlannedPrResponse, RepointPlannedPrRequest, RepointPlannedPrResponse,
    ResolveStackBaseRequest, ResolveStackBaseResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::PrStackHandler;
use tddy_task::IdleTimeoutTracker;

/// The eight PR-stack RPCs: the caller's identity, the sessions and stack plans under
/// `tddy_data_dir`, the GitHub token a PR status is read with, and the peer an orchestrator one
/// host over is reached through.
// TODO(#carve 11): the fields are read by the moved bodies; drop this allow when they land.
#[allow(dead_code)]
pub struct PrStackRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    github_token_store: Option<Arc<dyn GitHubTokenStore>>,
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
}

impl PrStackRpcHandler {
    /// A handler sharing `host`'s state — the same token store, idle tracker and peer routing.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        // TODO(#carve 11): clone the host's PR-stack fields once the host exposes them.
        let _ = host;
        todo!("PrStackRpcHandler::from_host")
    }
}

#[async_trait]
impl PrStackHandler for PrStackRpcHandler {
    async fn add_planned_pr(
        &self,
        _request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status> {
        todo!("PrStackRpcHandler::add_planned_pr")
    }

    async fn get_pr_status(
        &self,
        _request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        todo!("PrStackRpcHandler::get_pr_status")
    }

    async fn query_branch(
        &self,
        _request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        todo!("PrStackRpcHandler::query_branch")
    }

    async fn resolve_stack_base(
        &self,
        _request: Request<ResolveStackBaseRequest>,
    ) -> Result<Response<ResolveStackBaseResponse>, Status> {
        todo!("PrStackRpcHandler::resolve_stack_base")
    }

    async fn link_stack_node(
        &self,
        _request: Request<LinkStackNodeRequest>,
    ) -> Result<Response<LinkStackNodeResponse>, Status> {
        todo!("PrStackRpcHandler::link_stack_node")
    }

    async fn repoint_planned_pr(
        &self,
        _request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status> {
        todo!("PrStackRpcHandler::repoint_planned_pr")
    }

    async fn reorder_planned_pr(
        &self,
        _request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status> {
        todo!("PrStackRpcHandler::reorder_planned_pr")
    }

    async fn pull_base_into_branch(
        &self,
        _request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        todo!("PrStackRpcHandler::pull_base_into_branch")
    }
}
