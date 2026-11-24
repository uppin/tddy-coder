//! `WatchWorkQueue`: the queue itself, live — its snapshot, then one per new revision, whoever wrote
//! it. Served by the daemon that holds the queue; a client opens it on that host.

use std::path::Path;

use super::project_stacks::{
    referencing_sessions_by_stack, store_status, summary_of, work_queue_target, WorkQueueTarget,
};
use super::queue_changes::QueueChanges;
use super::WorkQueueRpcHandler;
use tddy_core::work_queue_store::WorkQueueRecord;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{WatchWorkQueueRequest, WorkQueueSnapshot};
use tddy_work_queue::rpc::WorkQueueWatchStream;

impl WorkQueueRpcHandler {
    pub(super) async fn watch_work_queue_rpc(
        &self,
        request: Request<WatchWorkQueueRequest>,
    ) -> Result<Response<WorkQueueWatchStream>, Status> {
        let req = request.into_inner();
        let os_user = self.authenticated_os_user(&req.session_token)?;
        let WorkQueueTarget::Address(address) = work_queue_target(req.queue.as_ref(), "")? else {
            unreachable!("a validated address resolves to an address")
        };
        let local = self.local_instance_id();
        if !address.daemon_instance_id.is_empty() && address.daemon_instance_id != local {
            return Err(Status::failed_precondition(format!(
                "work queue {} is held by host {}: watch it there",
                address.queue_id, address.daemon_instance_id
            )));
        }
        let sessions_base = self.sessions_base(&os_user)?;
        let store = self.stack_store(&sessions_base);
        // A queue this host does not hold is refused before anything is watched.
        store
            .read(&address.project_id, &address.queue_id)
            .map_err(store_status)?;
        // Watched before the snapshot is read, so a write landing during that read is heard.
        let mut changes =
            QueueChanges::of_record(&store.record_path(&address.queue_id).map_err(store_status)?)?;
        let first = store
            .read(&address.project_id, &address.queue_id)
            .map_err(store_status)?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let snapshot_of = move |record: &WorkQueueRecord| snapshot(record, &sessions_base, &local);
        let mut sent = first.stack.revision;
        if tx.send(snapshot_of(&first)).is_err() {
            return Ok(Response::new(rx.into()));
        }
        tokio::spawn(async move {
            loop {
                // Ends with the client: a dropped watch stops the file watch behind it.
                let changed = tokio::select! {
                    () = tx.closed() => return,
                    changed = changes.changed() => changed,
                };
                if changed.is_none() {
                    return;
                }
                // A queue removed while it is watched ends the watch with the store's reason.
                let record = match store.read(&address.project_id, &address.queue_id) {
                    Ok(record) => record,
                    Err(error) => {
                        let _ = tx.send(Err(store_status(error)));
                        return;
                    }
                };
                if record.stack.revision <= sent {
                    continue;
                }
                sent = record.stack.revision;
                if tx.send(snapshot_of(&record)).is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(rx.into()))
    }
}

/// `record` as one snapshot of the watch, summarized as `GetWorkQueue` summarizes it.
fn snapshot(
    record: &WorkQueueRecord,
    sessions_base: &Path,
    daemon_instance_id: &str,
) -> Result<WorkQueueSnapshot, Status> {
    let referencing = referencing_sessions_by_stack(sessions_base)
        .get(&record.queue_id)
        .cloned()
        .unwrap_or_default();
    Ok(WorkQueueSnapshot {
        revision: record.stack.revision,
        work_queue_json: serde_json::to_string(&record.stack)
            .map_err(|e| Status::internal(e.to_string()))?,
        summary: Some(summary_of(record, daemon_instance_id, referencing)),
    })
}
