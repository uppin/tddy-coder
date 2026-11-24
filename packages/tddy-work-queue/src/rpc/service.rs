use async_trait::async_trait;
use std::sync::Arc;

use super::WORK_QUEUE_SERVICE;

use tddy_service::proto::work_queue::{
    LandWorkQueueResponse, PullBaseIntoBranchRequest, PullBaseIntoBranchResponse,
    QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest, ReorderWorkItemResponse,
    RepointWorkItemRequest, RepointWorkItemResponse,
};

use tddy_service::proto::work_queue::LandWorkQueueRequest;

use tddy_service::proto::work_queue::PropagateWorkItemResponse;

use tddy_service::proto::work_queue::PropagateWorkItemRequest;

use tddy_service::proto::work_queue::ListWorkQueueSchedulesRequest;

use tddy_service::proto::work_queue::UnscheduleWorkItemRequest;

use tddy_service::proto::work_queue::ListWorkQueueSchedulesResponse;

use tddy_service::proto::work_queue::ScheduleWorkQueueWaveRequest;

use tddy_service::proto::work_queue::ScheduleWorkItemResponse;

use tddy_service::proto::work_queue::ScheduleWorkItemRequest;

use tddy_service::proto::work_queue::WorkQueueCommitCountsResponse;

use tddy_service::proto::work_queue::WorkItemOutboxResponse;

use tddy_service::proto::work_queue::WorkItemInboxResponse;

use tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest;

use tddy_service::proto::work_queue::WorkQueueWavesResponse;

use tddy_service::proto::work_queue::GetWorkQueueWavesRequest;

use tddy_service::proto::work_queue::WorkQueueGraphResponse;

use tddy_service::proto::work_queue::GetWorkQueueGraphRequest;

use tddy_service::proto::work_queue::SetWorkQueueDependenciesRequest;

use tddy_service::proto::work_queue::ClearWorkItemBadgeRequest;

use tddy_service::proto::work_queue::SetWorkItemBadgeRequest;

use tddy_service::proto::work_queue::SetWorkItemLabelsRequest;

use tddy_service::proto::work_queue::UnlinkWorkItemSessionRequest;

use tddy_service::proto::work_queue::LinkWorkItemSessionRequest;

use tddy_service::proto::work_queue::UnassignWorkItemPrRequest;

use tddy_service::proto::work_queue::WorkItemLinksResponse;

use tddy_service::proto::work_queue::AssignWorkItemPrRequest;

use tddy_service::proto::work_queue::GetWorkQueueResponse;

use tddy_service::proto::work_queue::GetWorkQueueRequest;

use tddy_service::proto::work_queue::ListProjectWorkQueuesResponse;

use tddy_service::proto::work_queue::ListProjectWorkQueuesRequest;

use tddy_service::proto::work_queue::LinkWorkItemResponse;

use tddy_service::proto::work_queue::LinkWorkItemRequest;

use tddy_service::proto::work_queue::ResolveWorkItemBaseResponse;

use tddy_service::proto::work_queue::ResolveWorkItemBaseRequest;

use tddy_service::proto::work_queue::GetPrStatusResponse;

use tddy_service::proto::work_queue::GetPrStatusRequest;

use tddy_service::proto::work_queue::AddWorkItemResponse;

use tddy_service::proto::work_queue::AddWorkItemRequest;

use tddy_rpc::Status;

use tddy_rpc::Response;

use tddy_service::proto::work_queue::StreamWorkQueueStatusRequest;

use tddy_rpc::Request;

use super::{WorkQueueStatusStream, WorkQueueWatchStream};

use tddy_service::proto::work_queue::WatchWorkQueueRequest;

use tddy_service::proto::work_queue::WorkQueueService;

use super::WorkQueueHandler;

/// Thin `WorkQueueService` adapter over a [`WorkQueueHandler`].
pub struct WorkQueueServiceImpl<H> {
    pub(crate) host: Arc<H>,
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

    type WatchWorkQueueStream = WorkQueueWatchStream;

    async fn watch_work_queue(
        &self,
        request: Request<WatchWorkQueueRequest>,
    ) -> Result<Response<Self::WatchWorkQueueStream>, Status> {
        self.host.watch_work_queue(request).await
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

    async fn set_work_queue_dependencies(
        &self,
        request: Request<SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.host.set_work_queue_dependencies(request).await
    }

    async fn get_work_queue_graph(
        &self,
        request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueGraphResponse>, Status> {
        self.host.get_work_queue_graph(request).await
    }

    async fn get_work_queue_waves(
        &self,
        request: Request<GetWorkQueueWavesRequest>,
    ) -> Result<Response<WorkQueueWavesResponse>, Status> {
        self.host.get_work_queue_waves(request).await
    }

    async fn get_work_item_inbox(
        &self,
        request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemInboxResponse>, Status> {
        self.host.get_work_item_inbox(request).await
    }

    async fn get_work_item_outbox(
        &self,
        request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemOutboxResponse>, Status> {
        self.host.get_work_item_outbox(request).await
    }

    async fn get_work_queue_commit_counts(
        &self,
        request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueCommitCountsResponse>, Status> {
        self.host.get_work_queue_commit_counts(request).await
    }

    async fn schedule_work_item(
        &self,
        request: Request<ScheduleWorkItemRequest>,
    ) -> Result<Response<ScheduleWorkItemResponse>, Status> {
        self.host.schedule_work_item(request).await
    }

    async fn schedule_work_queue_wave(
        &self,
        request: Request<ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        self.host.schedule_work_queue_wave(request).await
    }

    async fn unschedule_work_item(
        &self,
        request: Request<UnscheduleWorkItemRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        self.host.unschedule_work_item(request).await
    }

    async fn list_work_queue_schedules(
        &self,
        request: Request<ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        self.host.list_work_queue_schedules(request).await
    }

    async fn propagate_work_item(
        &self,
        request: Request<PropagateWorkItemRequest>,
    ) -> Result<Response<PropagateWorkItemResponse>, Status> {
        self.host.propagate_work_item(request).await
    }

    async fn land_work_queue(
        &self,
        request: Request<LandWorkQueueRequest>,
    ) -> Result<Response<LandWorkQueueResponse>, Status> {
        self.host.land_work_queue(request).await
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
