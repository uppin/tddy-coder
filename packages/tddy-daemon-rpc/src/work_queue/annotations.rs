//! `SetWorkItemLabels`, `SetWorkItemBadge` and `ClearWorkItemBadge` (`#workqueue` 7/14): the labels
//! and keyed badges on a work item.
//!
//! Each acts on the queue its address names, in this host's store (forwarded to the daemon the
//! address names when that is a peer), under the same access as the link calls, and answers the queue
//! after the change. A limit is a refusal naming the limit; nothing is truncated.

use super::links::{required, LinkCall};
use super::WorkQueueRpcHandler;
use tddy_core::changeset::BadgeTone;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    ClearWorkItemBadgeRequest, SetWorkItemBadgeRequest, SetWorkItemLabelsRequest,
    WorkItemLinksResponse,
};
use tddy_work_queue::item_ops::item_annotations::{self, AnnotationError, WorkItemBadge};

fn annotation_refusal(error: AnnotationError) -> Status {
    match error {
        AnnotationError::NoSuchItem(item) => Status::not_found(format!("item '{item}' not found")),
        AnnotationError::TooMany { limit } => {
            Status::invalid_argument(format!("too many: the limit is {limit}"))
        }
        AnnotationError::TooLong { limit } => {
            Status::invalid_argument(format!("too long: the limit is {limit} characters"))
        }
    }
}

/// A tone as the wire names it: one of four.
pub fn parse_tone(tone: &str) -> Result<BadgeTone, Status> {
    match tone.trim() {
        "info" => Ok(BadgeTone::Info),
        "success" => Ok(BadgeTone::Success),
        "warning" => Ok(BadgeTone::Warning),
        "danger" => Ok(BadgeTone::Danger),
        other => Err(Status::invalid_argument(format!(
            "tone '{other}' is not one of info, success, warning, danger"
        ))),
    }
}

impl WorkQueueRpcHandler {
    async fn annotation_call<R: prost::Message>(
        &self,
        method: &str,
        session_token: &str,
        queue: Option<&tddy_service::proto::work_queue::WorkQueueAddress>,
        req: &R,
    ) -> Result<Result<LinkCall, WorkItemLinksResponse>, Status> {
        let call = self.link_call(session_token, queue)?;
        match self
            .forward_addressed::<_, WorkItemLinksResponse>(
                method,
                &call.address.daemon_instance_id,
                req,
            )
            .await?
        {
            Some(answered) => Ok(Err(answered)),
            None => Ok(Ok(call)),
        }
    }

    pub(super) async fn set_work_item_labels_rpc(
        &self,
        request: Request<SetWorkItemLabelsRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = match self
            .annotation_call("SetWorkItemLabels", &req.session_token, req.queue.as_ref(), &req)
            .await?
        {
            Ok(call) => call,
            Err(answered) => return Ok(Response::new(answered)),
        };
        let item_id = required(&req.item_id, "item_id")?;
        let labels = req.labels.clone();
        self.edit_queue(
            &call,
            |queue| item_annotations::set_labels(queue, item_id, labels),
            annotation_refusal,
        )
    }

    pub(super) async fn set_work_item_badge_rpc(
        &self,
        request: Request<SetWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = match self
            .annotation_call("SetWorkItemBadge", &req.session_token, req.queue.as_ref(), &req)
            .await?
        {
            Ok(call) => call,
            Err(answered) => return Ok(Response::new(answered)),
        };
        let item_id = required(&req.item_id, "item_id")?;
        let badge = WorkItemBadge {
            key: required(&req.key, "key")?.to_string(),
            text: req.text.trim().to_string(),
            tone: parse_tone(&req.tone)?,
        };
        self.edit_queue(
            &call,
            |queue| item_annotations::set_badge(queue, item_id, badge),
            annotation_refusal,
        )
    }

    pub(super) async fn clear_work_item_badge_rpc(
        &self,
        request: Request<ClearWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = match self
            .annotation_call("ClearWorkItemBadge", &req.session_token, req.queue.as_ref(), &req)
            .await?
        {
            Ok(call) => call,
            Err(answered) => return Ok(Response::new(answered)),
        };
        let item_id = required(&req.item_id, "item_id")?;
        let key = required(&req.key, "key")?;
        self.edit_queue(
            &call,
            |queue| item_annotations::clear_badge(queue, item_id, key),
            annotation_refusal,
        )
    }
}
