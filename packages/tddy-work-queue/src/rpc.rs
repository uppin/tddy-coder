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
    AddWorkItemRequest, AddWorkItemResponse, AssignWorkItemPrRequest, ClearWorkItemBadgeRequest,
    GetPrStatusRequest, GetPrStatusResponse, GetWorkQueueRequest, GetWorkQueueResponse,
    LinkWorkItemRequest, LinkWorkItemResponse, LinkWorkItemSessionRequest,
    ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse, SetWorkItemBadgeRequest,
    SetWorkItemLabelsRequest, StreamWorkQueueStatusRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, WorkItemLinksResponse, WorkQueueService, WorkQueueStatusView,
};

/// The frames of `StreamWorkQueueStatus`: a channel the handler's task feeds, ended when either side
/// goes away.
pub struct WorkQueueStatusStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<WorkQueueStatusView, Status>>,
}

impl From<tokio::sync::mpsc::UnboundedReceiver<Result<WorkQueueStatusView, Status>>>
    for WorkQueueStatusStream
{
    fn from(rx: tokio::sync::mpsc::UnboundedReceiver<Result<WorkQueueStatusView, Status>>) -> Self {
        Self { rx }
    }
}

impl futures_util::Stream for WorkQueueStatusStream {
    type Item = Result<WorkQueueStatusView, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl std::fmt::Debug for WorkQueueStatusStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkQueueStatusStream")
    }
}

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
    /// `#workqueue` 6/14: assign an existing PR to a work item.
    async fn assign_work_item_pr(
        &self,
        request: Request<AssignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status>;

    /// `#workqueue` 6/14: remove a work item's assigned PR.
    async fn unassign_work_item_pr(
        &self,
        request: Request<UnassignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status>;

    /// `#workqueue` 6/14: link a tddy session to a work item.
    async fn link_work_item_session(
        &self,
        request: Request<LinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status>;

    /// `#workqueue` 6/14: unlink a tddy session from a work item.
    async fn unlink_work_item_session(
        &self,
        request: Request<UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status>;

    /// `#workqueue` 7/14: SetWorkItemLabels.
    async fn set_work_item_labels(
        &self,
        _request: Request<SetWorkItemLabelsRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("SetWorkItemLabels"))
    }

    /// `#workqueue` 7/14: SetWorkItemBadge.
    async fn set_work_item_badge(
        &self,
        _request: Request<SetWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("SetWorkItemBadge"))
    }

    /// `#workqueue` 7/14: ClearWorkItemBadge.
    async fn clear_work_item_badge(
        &self,
        _request: Request<ClearWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("ClearWorkItemBadge"))
    }

    /// `#workqueue` 6/14: the queue's PR state and check counts, live from the GitHub inbox.
    async fn stream_work_queue_status(
        &self,
        request: Request<StreamWorkQueueStatusRequest>,
    ) -> Result<Response<WorkQueueStatusStream>, Status>;
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
    type StreamWorkQueueStatusStream = WorkQueueStatusStream;

    async fn stream_work_queue_status(
        &self,
        request: Request<StreamWorkQueueStatusRequest>,
    ) -> Result<Response<Self::StreamWorkQueueStatusStream>, Status> {
        self.host.stream_work_queue_status(request).await
    }

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
    async fn assign_work_item_pr(
        &self,
        request: Request<AssignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.assign_work_item_pr(request).await
    }

    async fn unassign_work_item_pr(
        &self,
        request: Request<UnassignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.unassign_work_item_pr(request).await
    }

    async fn link_work_item_session(
        &self,
        request: Request<LinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.link_work_item_session(request).await
    }

    async fn unlink_work_item_session(
        &self,
        request: Request<UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.unlink_work_item_session(request).await
    }

    async fn set_work_item_labels(
        &self,
        request: Request<SetWorkItemLabelsRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.set_work_item_labels(request).await
    }

    async fn set_work_item_badge(
        &self,
        request: Request<SetWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.set_work_item_badge(request).await
    }

    async fn clear_work_item_badge(
        &self,
        request: Request<ClearWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.clear_work_item_badge(request).await
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
