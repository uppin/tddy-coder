//! Family P — [`tddy_session_lifecycle::WorkQueueHandler`], answered by [`WorkQueueRpcHandler`].

use super::WorkQueueRpcHandler;
use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetWorkQueueRequest, GetWorkQueueResponse, LinkWorkItemRequest, LinkWorkItemResponse,
    ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse,
};
use tddy_service::proto::work_queue::{
    AssignWorkItemPrRequest, ClearWorkItemBadgeRequest, LinkWorkItemSessionRequest,
    SetWorkItemBadgeRequest, SetWorkItemLabelsRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, WorkItemLinksResponse,
};
use tddy_session_lifecycle::WorkQueueHandler;

#[async_trait]
impl WorkQueueHandler for WorkQueueRpcHandler {
    async fn add_work_item(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status> {
        self.add_work_item_rpc(request).await
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        self.get_pr_status_rpc(request).await
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        self.query_branch_rpc(request).await
    }

    /// What a child spawn's branch bases off, answered by the daemon that owns the stack parent.
    ///
    /// The whole point of the call is *where* it is served. A `work_queue_parent` is a bare session id
    /// and the base is read out of that session's own `changeset.yaml`, so a daemon resolving one
    /// it does not hold reads an absent file — not "this branch was not planned", but "no such
    /// session", which refuses a spawn over an orchestrator that exists one host over. So the
    /// question is routed to the named daemon first, and answered from local disk only when this
    /// daemon is the one that owns it.
    ///
    /// Routed **before** the caller is authenticated, like the roster RPCs: a token is verified by
    /// the daemon that serves the call, and a peer's user mapping is not this one's to judge.
    async fn resolve_work_item_base(
        &self,
        request: Request<ResolveWorkItemBaseRequest>,
    ) -> Result<Response<ResolveWorkItemBaseResponse>, Status> {
        self.resolve_work_item_base_rpc(request).await
    }

    /// Record a child session and the branch it created on a planned node of a pr-stack
    /// orchestrator, answered by the daemon that owns that orchestrator.
    ///
    /// The write half of [`Self::resolve_work_item_base`], and routed for the same reason: a planned
    /// node lives in its orchestrator's own `changeset.yaml`, so only the daemon holding that
    /// session can write it. A child spawned on another host writing its *own* sessions tree finds
    /// no such session and writes nothing at all — the node keeps neither a branch nor a child, and
    /// every descendant then stays unspawnable because
    /// [`tddy_core::changeset::WorkQueue::base_ref_for_spawn`] gates on a parent owning a branch (D35).
    ///
    /// Routed **before** the caller is authenticated, like every other peer-routed RPC: a token is
    /// verified by the daemon that serves the call, and a peer's user mapping is not this one's to
    /// judge.
    ///
    /// Nothing here is guessed. An unnamed node is refused rather than derived from the branch
    /// (D34), and a node the plan does not hold is `not_found` with nothing written — silently
    /// succeeding against an unchanged plan is precisely the bug this RPC exists to remove.
    async fn link_work_item(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status> {
        self.link_work_item_rpc(request).await
    }

    async fn repoint_work_item(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status> {
        self.repoint_work_item_rpc(request).await
    }

    /// Move one planned node up or down the operator's reading order.
    ///
    /// Touches nothing but `WorkItem.display_order`: the dependency graph is a different fact, and
    /// keeping the two independent is the whole reason the field exists. Moving past either end is a
    /// successful no-op — the control at the end of the list is inert, not wrong.
    async fn reorder_work_item(
        &self,
        request: Request<ReorderWorkItemRequest>,
    ) -> Result<Response<ReorderWorkItemResponse>, Status> {
        self.reorder_work_item_rpc(request).await
    }

    async fn stream_work_queue_status(
        &self,
        request: Request<tddy_service::proto::work_queue::StreamWorkQueueStatusRequest>,
    ) -> Result<Response<tddy_work_queue::rpc::WorkQueueStatusStream>, Status> {
        self.stream_work_queue_status_rpc(request).await
    }

    async fn assign_work_item_pr(
        &self,
        request: Request<AssignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.assign_work_item_pr_rpc(request).await
    }

    async fn unassign_work_item_pr(
        &self,
        request: Request<UnassignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.unassign_work_item_pr_rpc(request).await
    }

    async fn link_work_item_session(
        &self,
        request: Request<LinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.link_work_item_session_rpc(request).await
    }

    async fn unlink_work_item_session(
        &self,
        request: Request<UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.unlink_work_item_session_rpc(request).await
    }

    async fn set_work_item_labels(
        &self,
        request: Request<SetWorkItemLabelsRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_item_labels_rpc(request).await
    }

    async fn set_work_item_badge(
        &self,
        request: Request<SetWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_item_badge_rpc(request).await
    }

    async fn clear_work_item_badge(
        &self,
        request: Request<ClearWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.clear_work_item_badge_rpc(request).await
    }

    async fn set_work_queue_dependencies(
        &self,
        request: Request<tddy_service::proto::work_queue::SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_queue_dependencies_rpc(request).await
    }

    async fn get_work_queue_graph(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueGraphResponse>, Status> {
        self.get_work_queue_graph_rpc(request).await
    }

    async fn get_work_queue_waves(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueWavesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueWavesResponse>, Status> {
        self.get_work_queue_waves_rpc(request).await
    }

    async fn get_work_item_inbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemInboxResponse>, Status> {
        self.get_work_item_inbox_rpc(request).await
    }

    async fn get_work_item_outbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemOutboxResponse>, Status> {
        self.get_work_item_outbox_rpc(request).await
    }

    async fn get_work_queue_commit_counts(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueCommitCountsResponse>, Status>
    {
        self.get_work_queue_commit_counts_rpc(request).await
    }

    async fn schedule_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::ScheduleWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ScheduleWorkItemResponse>, Status> {
        self.schedule_work_item_rpc(request).await
    }

    async fn schedule_work_queue_wave(
        &self,
        request: Request<tddy_service::proto::work_queue::ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.schedule_work_queue_wave_rpc(request).await
    }

    async fn unschedule_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::UnscheduleWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.unschedule_work_item_rpc(request).await
    }

    async fn list_work_queue_schedules(
        &self,
        request: Request<tddy_service::proto::work_queue::ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.list_work_queue_schedules_rpc(request).await
    }

    async fn propagate_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::PropagateWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::PropagateWorkItemResponse>, Status> {
        self.propagate_work_item_rpc(request).await
    }

    async fn land_work_queue(
        &self,
        request: Request<tddy_service::proto::work_queue::LandWorkQueueRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::LandWorkQueueResponse>, Status> {
        self.land_work_queue_rpc(request).await
    }

    /// Take a base branch's commits into a planned node's branch, inside that node's own worktree,
    /// and push the result.
    ///
    /// A mutation, so an unknown checkout is a precondition failure rather than a degraded leg — the
    /// pull has nowhere to happen. The branch is then re-resolved **uncached**: the refs it compares
    /// have just moved, and the point of returning a resolution at all is that the row repaints
    /// without waiting for the next poll tick.
    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        self.pull_base_into_branch_rpc(request).await
    }

    /// See `project_stacks.rs`, which holds the project-stack RPCs (`#orchestrators` 9/11).
    async fn list_project_work_queues(
        &self,
        request: Request<ListProjectWorkQueuesRequest>,
    ) -> Result<Response<ListProjectWorkQueuesResponse>, Status> {
        self.list_project_work_queues_rpc(request).await
    }

    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status> {
        self.get_work_queue_rpc(request).await
    }
}
