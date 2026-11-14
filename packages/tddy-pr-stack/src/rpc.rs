//! `pr_stack.PrStackService` — family P's handler trait, its service adapter and its transport
//! entry.
//!
//! The trait is defined here, beside the data model it serves, rather than by the daemon crate that
//! implements it. That lets the session lifecycle name `PrStackHandler` — for the session-start
//! paths that link or resolve a stack node — without naming the crate above it that holds the
//! bodies. [`PR_STACK_SERVICE`] moved here with the entry builder that registers it:
//! `tddy-workflow-recipes`, its old home, depends on this crate, so this crate cannot name it there.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetProjectStackRequest, GetProjectStackResponse, LinkStackNodeRequest, LinkStackNodeResponse,
    ListProjectStacksRequest, ListProjectStacksResponse, PrStackService, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderPlannedPrRequest,
    ReorderPlannedPrResponse, RepointPlannedPrRequest, RepointPlannedPrResponse,
    ResolveStackBaseRequest, ResolveStackBaseResponse,
};

/// The coordinate family P is served on — `pr_stack.PrStackService`.
pub const PR_STACK_SERVICE: &str = "pr_stack.PrStackService";

/// What the ten PR-stack RPCs need from the host process.
#[async_trait]
pub trait PrStackHandler: Send + Sync {
    async fn add_planned_pr(
        &self,
        request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status>;

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status>;

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status>;

    async fn resolve_stack_base(
        &self,
        request: Request<ResolveStackBaseRequest>,
    ) -> Result<Response<ResolveStackBaseResponse>, Status>;

    async fn link_stack_node(
        &self,
        request: Request<LinkStackNodeRequest>,
    ) -> Result<Response<LinkStackNodeResponse>, Status>;

    async fn repoint_planned_pr(
        &self,
        request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status>;

    async fn reorder_planned_pr(
        &self,
        request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status>;

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status>;

    /// A project's stacks on every host that holds the project, with every host that did not
    /// answer named (`#orchestrators` 9/11).
    async fn list_project_stacks(
        &self,
        request: Request<ListProjectStacksRequest>,
    ) -> Result<Response<ListProjectStacksResponse>, Status>;

    /// One project stack by its address, served by the host whose store holds it
    /// (`#orchestrators` 9/11).
    async fn get_project_stack(
        &self,
        request: Request<GetProjectStackRequest>,
    ) -> Result<Response<GetProjectStackResponse>, Status>;
}

/// Thin `PrStackService` adapter over a [`PrStackHandler`].
pub struct PrStackServiceImpl<H> {
    host: Arc<H>,
}

impl<H> PrStackServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: PrStackHandler + 'static> PrStackService for PrStackServiceImpl<H> {
    async fn add_planned_pr(
        &self,
        request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status> {
        self.host.add_planned_pr(request).await
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        self.host.get_pr_status(request).await
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        self.host.query_branch(request).await
    }

    async fn resolve_stack_base(
        &self,
        request: Request<ResolveStackBaseRequest>,
    ) -> Result<Response<ResolveStackBaseResponse>, Status> {
        self.host.resolve_stack_base(request).await
    }

    async fn link_stack_node(
        &self,
        request: Request<LinkStackNodeRequest>,
    ) -> Result<Response<LinkStackNodeResponse>, Status> {
        self.host.link_stack_node(request).await
    }

    async fn repoint_planned_pr(
        &self,
        request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status> {
        self.host.repoint_planned_pr(request).await
    }

    async fn reorder_planned_pr(
        &self,
        request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status> {
        self.host.reorder_planned_pr(request).await
    }

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        self.host.pull_base_into_branch(request).await
    }

    async fn list_project_stacks(
        &self,
        request: Request<ListProjectStacksRequest>,
    ) -> Result<Response<ListProjectStacksResponse>, Status> {
        self.host.list_project_stacks(request).await
    }

    async fn get_project_stack(
        &self,
        request: Request<GetProjectStackRequest>,
    ) -> Result<Response<GetProjectStackResponse>, Status> {
        self.host.get_project_stack(request).await
    }
}

/// Registers `pr_stack.PrStackService` on the RPC transport.
#[must_use]
pub fn build_pr_stack_entry<S>(service: S) -> tddy_rpc::ServiceEntry
where
    S: PrStackService + Send + Sync + 'static,
{
    use std::sync::Arc as StdArc;
    use tddy_rpc::RpcService;
    use tddy_service::proto::pr_stack::PrStackServiceServer;

    tddy_rpc::ServiceEntry {
        name: PR_STACK_SERVICE,
        service: StdArc::new(PrStackServiceServer::new(service)) as StdArc<dyn RpcService>,
    }
}
