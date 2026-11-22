//! `StreamWorkQueueStatus` (`#workqueue` 6/14): each item's PR state and check counts, read through
//! the daemon's GitHub inbox — a snapshot, then a new one whenever the inbox reports a PR or checks
//! change in the queue's repository. The browser never polls GitHub.

use super::project_stacks::{store_status, work_queue_target, WorkQueueTarget};
use super::WorkQueueRpcHandler;
use tddy_github_inbox::EventKind;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    PrStatusView, StreamWorkQueueStatusRequest, WorkItemStatus, WorkQueueStatusView,
};
use tddy_work_queue::inbox_reads::{item_check_summaries, pr_status_view_through_inbox};
use tddy_work_queue::rpc::WorkQueueStatusStream;

impl WorkQueueRpcHandler {
    pub(super) async fn stream_work_queue_status_rpc(
        &self,
        request: Request<StreamWorkQueueStatusRequest>,
    ) -> Result<Response<WorkQueueStatusStream>, Status> {
        let req = request.into_inner();
        let os_user = self.authenticated_os_user(&req.session_token)?;
        let WorkQueueTarget::Address(address) = work_queue_target(req.queue.as_ref(), "")? else {
            unreachable!("a validated address resolves to an address")
        };
        let local = self.local_instance_id();
        if !address.daemon_instance_id.is_empty() && address.daemon_instance_id != local {
            // TODO: a queue another host holds is not streamed from here (unary calls forward;
            // streams do not yet) — docs/dev/todo/2026-11-22-work-queue-status-stream-is-local-only.md
            return Err(Status::unimplemented(
                "the status of a work queue another host holds is not streamed from here",
            ));
        }
        let sessions_base = self.sessions_base(&os_user)?;
        let store = self.stack_store(&sessions_base);
        store
            .read(&address.project_id, &address.queue_id)
            .map_err(store_status)?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let site = match self.pull_requests.site_for_session(
            &os_user,
            &address.queue_id,
            &address.project_id,
            &req.session_token,
        ) {
            Ok(site) => site,
            Err(status) => {
                // The queue is real, its GitHub is not readable: say so in-band, once.
                let _ = tx.send(Ok(WorkQueueStatusView {
                    items: Vec::new(),
                    unavailable_reason: status.message().to_string(),
                }));
                return Ok(Response::new(rx.into()));
            }
        };
        // Subscribed before the first read, so a change landing during it is not missed.
        let mut events = site
            .inbox
            .subscribe(&site.repo, &[EventKind::Pr, EventKind::Checks]);
        tokio::spawn(async move {
            loop {
                let Ok(record) = store.read(&address.project_id, &address.queue_id) else {
                    return;
                };
                let summaries = item_check_summaries(&site.inbox, &site.repo, &record.stack).await;
                let mut items = Vec::new();
                for item in &record.stack.nodes {
                    let branch = item
                        .pr
                        .as_ref()
                        .map(|pr| pr.head_branch.as_str())
                        .or(item.branch.as_deref());
                    let pr = match branch {
                        Some(branch) => {
                            pr_status_view_through_inbox(&site.inbox, &site.repo, branch).await
                        }
                        None => PrStatusView::default(),
                    };
                    let checks = summaries.iter().find(|s| s.item_id == item.node_id);
                    items.push(WorkItemStatus {
                        item_id: item.node_id.clone(),
                        pr: Some(pr),
                        has_checks: checks.is_some(),
                        checks_running: checks.map_or(0, |c| c.running),
                        checks_failed: checks.map_or(0, |c| c.failed),
                        checks_succeeded: checks.map_or(0, |c| c.succeeded),
                    });
                }
                let view = WorkQueueStatusView {
                    items,
                    unavailable_reason: String::new(),
                };
                if tx.send(Ok(view)).is_err() {
                    return;
                }
                // Ends with the client: a dropped subscription stops the inbox feed behind it.
                let event = tokio::select! {
                    () = tx.closed() => return,
                    event = events.next() => event,
                };
                if event.is_none() {
                    return;
                }
            }
        });
        Ok(Response::new(rx.into()))
    }
}
