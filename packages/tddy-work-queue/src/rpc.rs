//! `work_queue.WorkQueueService` — family P's handler trait, its service adapter and its transport
//! entry.
//!
//! The trait is defined here, beside the data model it serves, rather than by the daemon crate that
//! implements it. That lets the session lifecycle name `WorkQueueHandler` — for the session-start
//! paths that link or resolve a stack node — without naming the crate above it that holds the
//! bodies. [`WORK_QUEUE_SERVICE`] moved here with the entry builder that registers it:
//! `tddy-workflow-recipes`, its old home, depends on this crate, so this crate cannot name it there.

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, AssignWorkItemPrRequest, ClearWorkItemBadgeRequest,
    GetPrStatusRequest, GetPrStatusResponse, GetWorkItemCommitFlowRequest,
    GetWorkQueueGraphRequest, GetWorkQueueRequest, GetWorkQueueResponse, GetWorkQueueWavesRequest,
    LandWorkQueueRequest, LandWorkQueueResponse, LinkWorkItemRequest, LinkWorkItemResponse,
    LinkWorkItemSessionRequest, ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse,
    ListWorkQueueSchedulesRequest, ListWorkQueueSchedulesResponse, PropagateWorkItemRequest,
    PropagateWorkItemResponse, PullBaseIntoBranchRequest, PullBaseIntoBranchResponse,
    QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest, ReorderWorkItemResponse,
    RepointWorkItemRequest, RepointWorkItemResponse, ResolveWorkItemBaseRequest,
    ResolveWorkItemBaseResponse, ScheduleWorkItemRequest, ScheduleWorkItemResponse,
    ScheduleWorkQueueWaveRequest, SetWorkItemBadgeRequest, SetWorkItemLabelsRequest,
    SetWorkQueueDependenciesRequest, StreamWorkQueueStatusRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, UnscheduleWorkItemRequest, WatchWorkQueueRequest,
    WorkItemInboxResponse, WorkItemLinksResponse, WorkItemOutboxResponse,
    WorkQueueCommitCountsResponse, WorkQueueGraphResponse, WorkQueueSnapshot, WorkQueueStatusView,
    WorkQueueWavesResponse,
};

/// The frames of a server stream: a channel the handler's task feeds, ended when either side goes
/// away.
pub struct ChannelStream<T> {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<T, Status>>,
}

/// The frames of `StreamWorkQueueStatus`.
pub type WorkQueueStatusStream = ChannelStream<WorkQueueStatusView>;

/// The frames of `WatchWorkQueue`.
pub type WorkQueueWatchStream = ChannelStream<WorkQueueSnapshot>;

impl<T> From<tokio::sync::mpsc::UnboundedReceiver<Result<T, Status>>> for ChannelStream<T> {
    fn from(rx: tokio::sync::mpsc::UnboundedReceiver<Result<T, Status>>) -> Self {
        Self { rx }
    }
}

impl<T> futures_util::Stream for ChannelStream<T> {
    type Item = Result<T, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl<T> std::fmt::Debug for ChannelStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChannelStream")
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

    /// `#workqueue` 9/14: replace what an item needs (its stored dependency edges).
    async fn set_work_queue_dependencies(
        &self,
        _request: Request<SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("SetWorkQueueDependencies"))
    }

    /// `#workqueue` 9/14: the queue's lineage — line edges and stored dependency edges.
    async fn get_work_queue_graph(
        &self,
        _request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueGraphResponse>, Status> {
        Err(Status::unimplemented("GetWorkQueueGraph"))
    }

    /// `#workqueue` 10/14: the queue's waves, derived line and readiness.
    async fn get_work_queue_waves(
        &self,
        _request: Request<GetWorkQueueWavesRequest>,
    ) -> Result<Response<WorkQueueWavesResponse>, Status> {
        Err(Status::unimplemented("GetWorkQueueWaves"))
    }

    /// `#workqueue` 11/14: the commits an item is missing, with sources and conflict predictions.
    async fn get_work_item_inbox(
        &self,
        _request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemInboxResponse>, Status> {
        Err(Status::unimplemented("GetWorkItemInbox"))
    }

    /// `#workqueue` 11/14: what an item owes each successor.
    async fn get_work_item_outbox(
        &self,
        _request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemOutboxResponse>, Status> {
        Err(Status::unimplemented("GetWorkItemOutbox"))
    }

    /// `#workqueue` 11/14: every item's inbox, outbox and conflict counts, for the queue rows.
    async fn get_work_queue_commit_counts(
        &self,
        _request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueCommitCountsResponse>, Status> {
        Err(Status::unimplemented("GetWorkQueueCommitCounts"))
    }

    /// `#workqueue` 13/14: ScheduleWorkItem.
    async fn schedule_work_item(
        &self,
        _request: Request<ScheduleWorkItemRequest>,
    ) -> Result<Response<ScheduleWorkItemResponse>, Status> {
        Err(Status::unimplemented("ScheduleWorkItem"))
    }

    /// `#workqueue` 13/14: ScheduleWorkQueueWave.
    async fn schedule_work_queue_wave(
        &self,
        _request: Request<ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        Err(Status::unimplemented("ScheduleWorkQueueWave"))
    }

    /// `#workqueue` 13/14: UnscheduleWorkItem.
    async fn unschedule_work_item(
        &self,
        _request: Request<UnscheduleWorkItemRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        Err(Status::unimplemented("UnscheduleWorkItem"))
    }

    /// `#workqueue` 13/14: ListWorkQueueSchedules.
    async fn list_work_queue_schedules(
        &self,
        _request: Request<ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        Err(Status::unimplemented("ListWorkQueueSchedules"))
    }

    /// `#workqueue` 14/14: PropagateWorkItem.
    async fn propagate_work_item(
        &self,
        _request: Request<PropagateWorkItemRequest>,
    ) -> Result<Response<PropagateWorkItemResponse>, Status> {
        Err(Status::unimplemented("PropagateWorkItem"))
    }

    /// `#workqueue` 14/14: LandWorkQueue.
    async fn land_work_queue(
        &self,
        _request: Request<LandWorkQueueRequest>,
    ) -> Result<Response<LandWorkQueueResponse>, Status> {
        Err(Status::unimplemented("LandWorkQueue"))
    }

    /// `#workqueue` 6/14: the queue's PR state and check counts, live from the GitHub inbox.
    async fn stream_work_queue_status(
        &self,
        request: Request<StreamWorkQueueStatusRequest>,
    ) -> Result<Response<WorkQueueStatusStream>, Status>;

    /// The queue itself, live: its snapshot, then one per new revision, whoever wrote it.
    async fn watch_work_queue(
        &self,
        request: Request<WatchWorkQueueRequest>,
    ) -> Result<Response<WorkQueueWatchStream>, Status>;
}

mod service;
pub use service::{build_pr_stack_entry, WorkQueueServiceImpl};

#[cfg(test)]
mod tests {
    use super::WORK_QUEUE_SERVICE;

    #[test]
    fn names_the_service_family_the_stack_handler_is_registered_under() {
        assert_eq!(WORK_QUEUE_SERVICE, "work_queue.WorkQueueService");
    }
}
