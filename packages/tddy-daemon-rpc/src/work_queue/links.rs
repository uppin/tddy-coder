//! `AssignWorkItemPr`, `UnassignWorkItemPr`, `LinkWorkItemSession` and `UnlinkWorkItemSession`
//! (`#workqueue` 6/14): an existing PR, and tddy sessions, linked to a work item.
//!
//! Each acts on the queue its address names, in this host's store (forwarded to the daemon the
//! address names when that is a peer), and answers the queue after the change. Assigning reads the
//! PR through the daemon's GitHub inbox with the project's credential — the number alone is what the
//! caller supplies, so the item records the PR's own URL and head branch, never a client's claim.

use super::project_stacks::{store_status, work_queue_target, WorkQueueTarget};
use super::WorkQueueRpcHandler;
use tddy_core::changeset::{AssignedPr, LinkedSession, WorkQueue};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    AssignWorkItemPrRequest, LinkWorkItemSessionRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, WorkItemLinksResponse, WorkQueueAddress,
};
use tddy_work_queue::item_ops::item_links::{self, ItemLinkError};

/// A refused link edit as the RPC status its kind calls for, worded by the error itself.
fn link_refusal(error: ItemLinkError) -> Status {
    let message = error.to_string();
    match error {
        ItemLinkError::NoSuchItem(_) | ItemLinkError::NotLinked { .. } => {
            Status::not_found(message)
        }
        ItemLinkError::BranchMismatch { .. } => Status::failed_precondition(message),
    }
}

fn required<'a>(value: &'a str, what: &str) -> Result<&'a str, Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(format!("{what} is required")));
    }
    Ok(value)
}

/// What every link call resolves before it acts: the caller's OS user and the queue's address.
struct LinkCall {
    os_user: String,
    address: WorkQueueAddress,
}

impl WorkQueueRpcHandler {
    /// Authenticate the caller and resolve the queue's address. The address is the only way to name a
    /// queue here: a queue is its owning session's (`#workqueue` 5/14), and the web view opens one by
    /// address.
    fn link_call(
        &self,
        session_token: &str,
        queue: Option<&WorkQueueAddress>,
    ) -> Result<LinkCall, Status> {
        let os_user = self.authenticated_os_user(session_token)?;
        let WorkQueueTarget::Address(address) = work_queue_target(queue, "")? else {
            unreachable!("a validated address resolves to an address")
        };
        Ok(LinkCall { os_user, address })
    }

    /// Apply `edit` to the queue in this host's store and answer the queue as written. A refused edit
    /// writes nothing.
    fn edit_links(
        &self,
        call: &LinkCall,
        edit: impl FnOnce(&mut WorkQueue) -> Result<(), ItemLinkError>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let sessions_base = self.sessions_base(&call.os_user)?;
        let store = self.stack_store(&sessions_base);
        let mut refused = None;
        let updated = store
            .update(&call.address.project_id, &call.address.queue_id, |queue| {
                // Edited on a copy, so a refusal leaves the stored queue exactly as it was.
                let mut edited = queue.clone();
                match edit(&mut edited) {
                    Ok(()) => *queue = edited,
                    Err(error) => refused = Some(error),
                }
            })
            .map_err(store_status)?;
        if let Some(error) = refused {
            return Err(link_refusal(error));
        }
        let work_queue_json =
            serde_json::to_string(&updated.stack).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(WorkItemLinksResponse { work_queue_json }))
    }

    pub(super) async fn assign_work_item_pr_rpc(
        &self,
        request: Request<AssignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemLinksResponse>(
                "AssignWorkItemPr",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = required(&req.item_id, "item_id")?;
        if req.pull_number == 0 {
            return Err(Status::invalid_argument("pull_number is required"));
        }
        // The PR is read through the inbox as the project's account; an unreadable one is refused
        // with the inbox's own reason, never recorded on faith.
        let site = self.pull_requests.site_for_session(
            &call.os_user,
            &call.address.queue_id,
            &call.address.project_id,
            &req.session_token,
        )?;
        let pr = site
            .inbox
            .pr(&site.repo, req.pull_number)
            .await
            .map_err(|error| {
                Status::failed_precondition(format!(
                    "pull request #{} could not be read: {error:?}",
                    req.pull_number
                ))
            })?
            .value;
        let assigned = AssignedPr {
            number: pr.number,
            url: pr.url,
            head_branch: pr.head_branch,
        };
        self.edit_links(&call, |queue| {
            item_links::assign_pr(queue, item_id, assigned)
        })
    }

    pub(super) async fn unassign_work_item_pr_rpc(
        &self,
        request: Request<UnassignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemLinksResponse>(
                "UnassignWorkItemPr",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = required(&req.item_id, "item_id")?;
        self.edit_links(&call, |queue| item_links::unassign_pr(queue, item_id))
    }

    pub(super) async fn link_work_item_session_rpc(
        &self,
        request: Request<LinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemLinksResponse>(
                "LinkWorkItemSession",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = required(&req.item_id, "item_id")?;
        let session = LinkedSession {
            session_id: required(&req.session_id, "session_id")?.to_string(),
            daemon_instance_id: req.daemon_instance_id.trim().to_string(),
        };
        self.edit_links(&call, |queue| {
            item_links::link_session(queue, item_id, session)
        })
    }

    pub(super) async fn unlink_work_item_session_rpc(
        &self,
        request: Request<UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemLinksResponse>(
                "UnlinkWorkItemSession",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = required(&req.item_id, "item_id")?;
        let session_id = required(&req.session_id, "session_id")?;
        self.edit_links(&call, |queue| {
            item_links::unlink_session(queue, item_id, session_id)
        })
    }
}
