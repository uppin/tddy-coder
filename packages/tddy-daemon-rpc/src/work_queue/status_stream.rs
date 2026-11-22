//! `StreamWorkQueueStatus` (`#workqueue` 6/14): each item's PR state and check counts, read through
//! the daemon's GitHub inbox — a snapshot, then a new one whenever the inbox reports a PR or checks
//! change in the queue's repository. The browser never polls GitHub.

use super::chain_health::{leak_views, line_view};
use super::project_stacks::{store_status, work_queue_target, WorkQueueTarget};
use super::WorkQueueRpcHandler;
use tddy_github_inbox::EventKind;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{StreamWorkQueueStatusRequest, WorkQueueStatusView};
use tddy_work_queue::chain_health_reads::queue_chain_health;
use tddy_work_queue::inbox_reads::work_queue_status_view;
use tddy_work_queue::lineage::line_edges_through_inbox;
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
                    schedules: Vec::new(),
                }));
                return Ok(Response::new(rx.into()));
            }
        };
        // The trunk the line edges end at, and the checkout the behind counts are read in. Without
        // them an item carries no chain health: the rest of its status is unaffected.
        // The schedules' conditions are re-evaluated with every update as well (`#workqueue` 13/14);
        // they say why when the trunk is unreadable.
        let trunk_read = self
            .link_call(&req.session_token, req.queue.as_ref())
            .and_then(|call| self.read_queue_and_trunk(&call))
            .map(|(_, _, trunk)| trunk)
            .map_err(|status| status.message().to_string());
        let trunk = trunk_read.as_ref().ok().cloned();
        let pull_requests = std::sync::Arc::clone(&self.pull_requests);
        let session_token = req.session_token.clone();
        let os_user_for_schedules = os_user.clone();
        let git_env_source = self.git_env_source();
        let repo_root = tddy_core::repo_root_for_session(
            &tddy_core::session_lifecycle::unified_session_dir_path(
                &sessions_base,
                &address.queue_id,
            ),
        );
        // Subscribed before the first read, so a change landing during it is not missed.
        let mut events = site.inbox.subscribe(
            &site.repo,
            &[
                EventKind::Pr,
                EventKind::Checks,
                EventKind::Comments,
                EventKind::Push,
            ],
        );
        tokio::spawn(async move {
            loop {
                // A queue deleted while it is viewed ends the stream with the store's reason.
                let record = match store.read(&address.project_id, &address.queue_id) {
                    Ok(record) => record,
                    Err(error) => {
                        let _ = tx.send(Err(store_status(error)));
                        return;
                    }
                };
                let mut view = work_queue_status_view(&site.inbox, &site.repo, &record.stack).await;
                if let Some(trunk) = &trunk {
                    let line =
                        line_edges_through_inbox(&site.inbox, &site.repo, &record.stack, trunk)
                            .await;
                    let health = queue_chain_health(
                        &site.inbox,
                        &site.repo,
                        &record.stack,
                        &line,
                        repo_root.clone(),
                    )
                    .await;
                    for item in &mut view.items {
                        if let Some(h) = health.iter().find(|h| h.item_id == item.item_id) {
                            item.leaks = leak_views(&h.leaks);
                            item.line = h.line.as_ref().map(line_view);
                        }
                    }
                }
                view.schedules = match &trunk_read {
                    Ok(trunk) => {
                        let (views, _) = super::schedules::ScheduleCtx {
                            store: &store,
                            pull_requests: pull_requests.as_ref(),
                            os_user: &os_user_for_schedules,
                            sessions_base: &sessions_base,
                            project_id: &address.project_id,
                            queue_id: &address.queue_id,
                            caller_session_id: &address.queue_id,
                            session_token: &session_token,
                            trunk,
                            armed_by: &os_user_for_schedules,
                        }
                        .evaluate_views(&record.stack)
                        .await;
                        // A schedule whose conditions all hold fires, once (`#workqueue` 14/14).
                        super::land::fire_met_in_background(
                            super::land::DaemonFireSchedule {
                                source: git_env_source.clone(),
                                store: store.clone(),
                                sessions_base: sessions_base.clone(),
                                os_user: os_user.clone(),
                                address: address.clone(),
                                session_token: session_token.clone(),
                            },
                            views.clone(),
                        );
                        views.into_iter().map(super::schedules::status_view).collect()
                    }
                    Err(reason) => super::schedules::unreadable_views(&record.stack, reason),
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
