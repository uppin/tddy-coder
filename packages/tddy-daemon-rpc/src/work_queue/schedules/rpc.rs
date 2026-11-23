//! The four schedule RPCs and the tools' JSON answers.

use super::*;

use serde_json::{json, Value};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    ListWorkQueueSchedulesRequest, ListWorkQueueSchedulesResponse, ScheduleStatusView,
    ScheduleWorkItemRequest, ScheduleWorkItemResponse, ScheduleWorkQueueWaveRequest,
    UnscheduleWorkItemRequest,
};

use super::super::WorkQueueRpcHandler;

/// The tools' answers, as the wire's views in JSON.
pub(crate) mod json {
    use super::*;

    pub(crate) fn schedule(view: ScheduleStatusView) -> Value {
        json!({
            "item_id": view.item_id,
            "kind": view.kind,
            "skip_review": view.skip_review,
            "armed_at": view.armed_at,
            "armed_by": view.armed_by,
            "status": view.status,
            "held_up_on": view.held_up_on,
            "conditions": view.conditions.into_iter().map(|c| json!({
                "name": c.name, "state": c.state, "detail": c.detail,
            })).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn schedules(views: Vec<ScheduleStatusView>, unavailable_reason: &str) -> Value {
        json!({
            "schedules": views.into_iter().map(schedule).collect::<Vec<_>>(),
            "unavailable_reason": unavailable_reason,
        })
    }
}

fn listed(
    (schedules, unavailable_reason): (Vec<ScheduleStatusView>, String),
) -> Response<ListWorkQueueSchedulesResponse> {
    Response::new(ListWorkQueueSchedulesResponse {
        schedules,
        unavailable_reason,
    })
}

impl WorkQueueRpcHandler {
    /// Resolve the call's queue on this host and run `act` over it; `None` when the address names
    /// another host (the caller forwards).
    async fn with_schedules<T>(
        &self,
        session_token: &str,
        queue: Option<&tddy_service::proto::work_queue::WorkQueueAddress>,
        act: impl for<'a> FnOnce(
            ScheduleCtx<'a>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, ScheduleOpError>> + Send + 'a>,
        >,
    ) -> Result<T, Status> {
        let call = self.link_call(session_token, queue)?;
        let (sessions_base, _, trunk) = self.read_queue_and_trunk(&call)?;
        let store = self.stack_store(&sessions_base);
        let ctx = ScheduleCtx {
            store: &store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &call.os_user,
            sessions_base: &sessions_base,
            project_id: &call.address.project_id,
            queue_id: &call.address.queue_id,
            caller_session_id: &call.address.queue_id,
            session_token,
            trunk: &trunk,
            armed_by: &call.os_user,
        };
        act(ctx).await.map_err(ScheduleOpError::into_status)
    }

    pub(in crate::work_queue) async fn schedule_work_item_rpc(
        &self,
        request: Request<ScheduleWorkItemRequest>,
    ) -> Result<Response<ScheduleWorkItemResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ScheduleWorkItemResponse>(
                "ScheduleWorkItem",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let (item_id, kind, skip_review) = (req.item_id.clone(), req.kind.clone(), req.skip_review);
        let schedule = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.arm_item(&item_id, &kind, skip_review).await })
            })
            .await?;
        self.fire_met_schedules(&call, &req.session_token).await;
        Ok(Response::new(ScheduleWorkItemResponse {
            schedule: Some(schedule),
        }))
    }

    pub(in crate::work_queue) async fn schedule_work_queue_wave_rpc(
        &self,
        request: Request<ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "ScheduleWorkQueueWave",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let (wave, skip_review) = (req.wave, req.skip_review);
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.arm_wave(wave, skip_review).await })
            })
            .await?;
        self.fire_met_schedules(&call, &req.session_token).await;
        Ok(listed(answered))
    }

    pub(in crate::work_queue) async fn unschedule_work_item_rpc(
        &self,
        request: Request<UnscheduleWorkItemRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "UnscheduleWorkItem",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.clone();
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.disarm(&item_id).await })
            })
            .await?;
        Ok(listed(answered))
    }

    pub(in crate::work_queue) async fn list_work_queue_schedules_rpc(
        &self,
        request: Request<ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "ListWorkQueueSchedules",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.list().await })
            })
            .await?;
        Ok(listed(answered))
    }
}
