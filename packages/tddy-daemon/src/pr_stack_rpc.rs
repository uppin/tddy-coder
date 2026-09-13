//! `pr_stack.PrStackService` adapter — family P.
//!
//! Handler bodies live in [`connection_service::svc_pr_stack_ports`]; this module owns the thin
//! `PrStackService` trait adapter (workflow-recipes cannot depend on `tddy-service` without a
//! cycle, so the adapter sits beside the daemon wiring).

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetPrStatusRequest, GetPrStatusResponse,
    LinkStackNodeRequest, LinkStackNodeResponse, PrStackService, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderPlannedPrRequest,
    ReorderPlannedPrResponse, RepointPlannedPrRequest, RepointPlannedPrResponse,
    ResolveStackBaseRequest, ResolveStackBaseResponse,
};

/// What the eight PR-stack RPCs need from the host process.
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
        name: tddy_workflow_recipes::PR_STACK_SERVICE,
        service: StdArc::new(PrStackServiceServer::new(service)) as StdArc<dyn RpcService>,
    }
}
