//! The files a work item carries, appended to a start request's attachments wherever that request
//! names the item.
//!
//! One rule, applied **below every spawn surface** — the web Start-session dialog, the agent's
//! `work_item_spawn_session`, any future one — because a child that differs by how it was started
//! is a bug the operator cannot see. Both materialization sites
//! (`svc_start_session_core`, `tool_session_spawn`) call [`work_item_files_for_start`] just before
//! `prepare_session_attachments`, so one materialization pass validates everything.
//!
//! Every file is attached **by reference**: a `HostDocumentRef` naming the queue owner's host and
//! session under the `WORK_QUEUE_FILE` scope (`<item_id>/<basename>`), which the materializer
//! fetches — streaming from a peer when the child starts on another host. Nothing is copied up
//! front, so a cross-host stack works unchanged. The list of files is read from the queue the
//! same way the base is: this daemon's handler, which answers an address it holds and forwards
//! one a peer holds.

use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    session_attachment::Source, HostDocumentRef, SessionAttachment,
};
use tddy_service::proto::types::HostDocumentScope;
use tddy_service::proto::work_queue::{GetWorkQueueRequest, WorkQueueAddress};
use tddy_work_queue::rpc::WorkQueueHandler;

use crate::launch_ports::LaunchSessions;

/// The item's files as start attachments, by reference; empty for a start that names no
/// work-queue parent or no item, and for an item that carries none.
///
/// A start naming an item whose queue cannot be read is refused: the operator asked for the item's
/// files, and starting without them silently is exactly the divergence this rule exists to prevent.
pub async fn work_item_files_for_start(
    work_queue: &dyn WorkQueueHandler,
    local_instance_id: &str,
    req: &tddy_service::proto::session::StartSessionRequest,
) -> Result<Vec<SessionAttachment>, Status> {
    let parent = req.work_queue_parent.trim();
    let item_id = req.work_item_id.trim();
    if parent.is_empty() || item_id.is_empty() {
        return Ok(Vec::new());
    }
    let named_host = req.work_queue_parent_daemon_instance_id.trim();
    let owner_host = if named_host.is_empty() {
        local_instance_id.to_string()
    } else {
        named_host.to_string()
    };

    let answer = work_queue
        .get_work_queue(Request::direct(GetWorkQueueRequest {
            session_token: req.session_token.clone(),
            stack: Some(WorkQueueAddress {
                project_id: req.project_id.trim().to_string(),
                queue_id: parent.to_string(),
                daemon_instance_id: named_host.to_string(),
            }),
        }))
        .await?
        .into_inner();
    let plan: serde_json::Value = serde_json::from_str(&answer.work_queue_json)
        .map_err(|e| Status::internal(format!("the queue's plan is unreadable: {e}")))?;
    let files = plan["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["node_id"] == serde_json::json!(item_id))
                .and_then(|node| node["files"].as_array())
                .cloned()
        })
        .unwrap_or_default();

    Ok(files
        .iter()
        .filter_map(|file| file["basename"].as_str())
        .map(|basename| SessionAttachment {
            basename: basename.to_string(),
            source: Some(Source::HostDocument(HostDocumentRef {
                daemon_instance_id: owner_host.clone(),
                scope: HostDocumentScope::WorkQueueFile as i32,
                session_id: parent.to_string(),
                project_id: String::new(),
                relative_path: format!("{item_id}/{basename}"),
            })),
        })
        .collect())
}

impl LaunchSessions {
    /// `req` with the work item's files appended to its attachments — the one seam every spawn
    /// path applies before it materializes anything, so a child of an item carries the item's
    /// files however it was started.
    pub(crate) async fn start_request_with_work_item_files(
        &self,
        req: &tddy_service::proto::session::StartSessionRequest,
    ) -> Result<tddy_service::proto::session::StartSessionRequest, Status> {
        let mut request = req.clone();
        request.attachments.extend(
            work_item_files_for_start(
                self.host.work_queue()?.as_ref(),
                &tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config),
                req,
            )
            .await?,
        );
        Ok(request)
    }
}
