//! `work_queue.WorkQueueService` — family P's handler trait, its service adapter and its transport
//! entry.
//!
//! The trait is defined here, beside the data model it serves, rather than by the daemon crate that
//! implements it. That lets the session lifecycle name `WorkQueueHandler` — for the session-start
//! paths that link or resolve a stack node — without naming the crate above it that holds the
//! bodies. [`WORK_QUEUE_SERVICE`] moved here with the entry builder that registers it:
//! `tddy-workflow-recipes`, its old home, depends on this crate, so this crate cannot name it there.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetWorkQueueRequest, GetWorkQueueResponse, LinkWorkItemRequest, LinkWorkItemResponse,
    ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse, WorkQueueService,
};

/// The coordinate family P is served on — `work_queue.WorkQueueService`.
pub const WORK_QUEUE_SERVICE: &str = "work_queue.WorkQueueService";

/// What the ten PR-stack RPCs need from the host process.
#[async_trait]
pub trait WorkQueueHandler: Send + Sync {
    async fn add_work_item(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status>;

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status>;

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status>;

    async fn resolve_work_item_base(
        &self,
        request: Request<ResolveWorkItemBaseRequest>,
    ) -> Result<Response<ResolveWorkItemBaseResponse>, Status>;

    async fn link_work_item(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status>;

    async fn repoint_work_item(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status>;

    async fn reorder_work_item(
        &self,
        request: Request<ReorderWorkItemRequest>,
    ) -> Result<Response<ReorderWorkItemResponse>, Status>;

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status>;

    /// A project's stacks on every host that holds the project, with every host that did not
    /// answer named (`#orchestrators` 9/11).
    async fn list_project_work_queues(
        &self,
        request: Request<ListProjectWorkQueuesRequest>,
    ) -> Result<Response<ListProjectWorkQueuesResponse>, Status>;

    /// One project stack by its address, served by the host whose store holds it
    /// (`#orchestrators` 9/11).
    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status>;
}

/// Thin `WorkQueueService` adapter over a [`WorkQueueHandler`].
pub struct WorkQueueServiceImpl<H> {
    host: Arc<H>,
}

impl<H> WorkQueueServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: WorkQueueHandler + 'static> WorkQueueService for WorkQueueServiceImpl<H> {
    async fn add_work_item(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status> {
        self.host.add_work_item(request).await
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

    async fn resolve_work_item_base(
        &self,
        request: Request<ResolveWorkItemBaseRequest>,
    ) -> Result<Response<ResolveWorkItemBaseResponse>, Status> {
        self.host.resolve_work_item_base(request).await
    }

    async fn link_work_item(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status> {
        self.host.link_work_item(request).await
    }

    async fn repoint_work_item(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status> {
        self.host.repoint_work_item(request).await
    }

    async fn reorder_work_item(
        &self,
        request: Request<ReorderWorkItemRequest>,
    ) -> Result<Response<ReorderWorkItemResponse>, Status> {
        self.host.reorder_work_item(request).await
    }

    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        self.host.pull_base_into_branch(request).await
    }

    async fn list_project_work_queues(
        &self,
        request: Request<ListProjectWorkQueuesRequest>,
    ) -> Result<Response<ListProjectWorkQueuesResponse>, Status> {
        self.host.list_project_work_queues(request).await
    }

    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status> {
        self.host.get_work_queue(request).await
    }
}

/// Registers `work_queue.WorkQueueService` on the RPC transport.
#[must_use]
pub fn build_pr_stack_entry<S>(service: S) -> tddy_rpc::ServiceEntry
where
    S: WorkQueueService + Send + Sync + 'static,
{
    use std::sync::Arc as StdArc;
    use tddy_rpc::RpcService;
    use tddy_service::proto::work_queue::WorkQueueServiceServer;

    tddy_rpc::ServiceEntry {
        name: WORK_QUEUE_SERVICE,
        service: StdArc::new(WorkQueueServiceServer::new(service)) as StdArc<dyn RpcService>,
    }
}

#[cfg(test)]
mod tests {
    use super::WORK_QUEUE_SERVICE;

    #[test]
    fn names_the_service_family_the_stack_handler_is_registered_under() {
        assert_eq!(WORK_QUEUE_SERVICE, "work_queue.WorkQueueService");
    }
}
