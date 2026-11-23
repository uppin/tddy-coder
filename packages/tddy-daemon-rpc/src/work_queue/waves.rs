//! `GetWorkQueueWaves` (`#workqueue` 10/14): the queue's waves, derived line and readiness, computed
//! from its stored dependency edges. Nothing is stored; a cycle is refused.

use super::lineage::lineage_refusal;
use super::project_stacks::store_status;
use super::WorkQueueRpcHandler;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    GetWorkQueueWavesRequest, ItemReadinessView, WaveView, WorkQueueWavesResponse,
};
use tddy_work_queue::{lineage, waves};

impl WorkQueueRpcHandler {
    pub(super) async fn get_work_queue_waves_rpc(
        &self,
        request: Request<GetWorkQueueWavesRequest>,
    ) -> Result<Response<WorkQueueWavesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkQueueWavesResponse>(
                "GetWorkQueueWaves",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let sessions_base = self.sessions_base(&call.os_user)?;
        let record = self
            .stack_store(&sessions_base)
            .read(&call.address.project_id, &call.address.queue_id)
            .map_err(store_status)?;
        let items: Vec<String> = record
            .stack
            .nodes
            .iter()
            .map(|item| item.node_id.clone())
            .collect();
        let derived = waves::queue_waves(
            &items,
            &lineage::dependencies_of(&record.stack),
            &waves::merged_items(&record.stack),
        )
        .map_err(lineage_refusal)?;
        Ok(Response::new(WorkQueueWavesResponse {
            waves: derived
                .waves
                .into_iter()
                .enumerate()
                .map(|(index, item_ids)| WaveView {
                    wave: index as u32 + 1,
                    item_ids,
                })
                .collect(),
            derived_line: derived.derived_line,
            readiness: derived
                .readiness
                .into_iter()
                .map(|(item_id, waits_on)| ItemReadinessView { item_id, waits_on })
                .collect(),
            out_of_order_at: derived.out_of_order_at.unwrap_or_default(),
        }))
    }
}
