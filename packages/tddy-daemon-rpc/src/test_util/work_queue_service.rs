use async_trait::async_trait;
use tddy_service::proto::work_queue::{
    GetWorkQueueRequest, GetWorkQueueResponse, ListProjectWorkQueuesRequest,
    ListProjectWorkQueuesResponse, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse,
};

use tddy_service::proto::work_queue::PullBaseIntoBranchResponse;

use tddy_service::proto::work_queue::PullBaseIntoBranchRequest;

use tddy_service::proto::work_queue::LinkWorkItemResponse;

use tddy_service::proto::work_queue::LinkWorkItemRequest;

use tddy_service::proto::work_queue::QueryBranchRequest;

use tddy_service::proto::work_queue::GetPrStatusResponse;

use tddy_service::proto::work_queue::GetPrStatusRequest;

use tddy_rpc::Status;

use tddy_service::proto::work_queue::AddWorkItemResponse;

use tddy_rpc::Response;

use tddy_service::proto::work_queue::AddWorkItemRequest;

use tddy_rpc::Request;

use super::TestDaemon;

use tddy_service::proto::work_queue::WorkQueueService;

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

    type WatchWorkQueueStream = tddy_work_queue::rpc::WorkQueueWatchStream;

    async fn watch_work_queue(
        &self,
        request: Request<tddy_service::proto::work_queue::WatchWorkQueueRequest>,
    ) -> Result<Response<Self::WatchWorkQueueStream>, Status> {
        self.handlers
            .work_queue_service()
            .watch_work_queue(request)
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

    async fn get_work_item_inbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemInboxResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_work_item_inbox(request)
            .await
    }

    async fn get_work_item_outbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemOutboxResponse>, Status> {
        self.handlers
            .work_queue_service()
            .get_work_item_outbox(request)
            .await
    }

    async fn get_work_queue_commit_counts(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueCommitCountsResponse>, Status>
    {
        self.handlers
            .work_queue_service()
            .get_work_queue_commit_counts(request)
            .await
    }

    async fn schedule_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::ScheduleWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ScheduleWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .schedule_work_item(request)
            .await
    }

    async fn schedule_work_queue_wave(
        &self,
        request: Request<tddy_service::proto::work_queue::ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.handlers
            .work_queue_service()
            .schedule_work_queue_wave(request)
            .await
    }

    async fn unschedule_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::UnscheduleWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.handlers
            .work_queue_service()
            .unschedule_work_item(request)
            .await
    }

    async fn list_work_queue_schedules(
        &self,
        request: Request<tddy_service::proto::work_queue::ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse>, Status>
    {
        self.handlers
            .work_queue_service()
            .list_work_queue_schedules(request)
            .await
    }

    async fn propagate_work_item(
        &self,
        request: Request<tddy_service::proto::work_queue::PropagateWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::PropagateWorkItemResponse>, Status> {
        self.handlers
            .work_queue_service()
            .propagate_work_item(request)
            .await
    }

    async fn land_work_queue(
        &self,
        request: Request<tddy_service::proto::work_queue::LandWorkQueueRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::LandWorkQueueResponse>, Status> {
        self.handlers
            .work_queue_service()
            .land_work_queue(request)
            .await
    }
}
