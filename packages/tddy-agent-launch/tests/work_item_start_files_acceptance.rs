//! Acceptance: the files a work item carries are appended to a start request's attachments
//! wherever that request names the item — below every spawn surface, so a child cannot differ by
//! how it was started.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`
//!
//! The helper answers `SessionAttachment`s **by reference**: a `HostDocumentRef` naming the queue
//! owner's host and session under the `WORK_QUEUE_FILE` scope, which the materializer fetches —
//! streaming from a peer when the child starts on another host. Nothing is copied up front.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_agent_launch::work_item_start_files::work_item_files_for_start;
use tddy_changeset::changeset::{WorkItem, WorkItemFile, WorkQueue};
use tddy_changeset::session_work_queue::SessionWorkQueues;
use tddy_core::work_queue_store::WorkQueueStore;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{session_attachment::Source, StartSessionRequest};
use tddy_service::proto::types::HostDocumentScope;
use tddy_service::proto::work_queue::{
    GetWorkQueueRequest, GetWorkQueueResponse, WorkQueueAddress,
};
use tddy_work_queue::rpc::WorkQueueHandler;

const PROJECT: &str = "proj-spawn-files";
const OWNER: &str = "queue-owner-spawn";
const ITEM: &str = "n1";
const OWNER_HOST: &str = "daemon-owner";

/// A data dir whose sessions tree holds `OWNER`'s queue with one item carrying two files, plus the
/// start request that names it.
struct AQueue {
    _data: tempfile::TempDir,
    request: StartSessionRequest,
}

fn a_queue_with_files() -> (AQueue, Arc<QueueOverStore>) {
    let data = tempfile::tempdir().expect("temp data dir");
    SessionWorkQueues::new(data.path().join("sessions"))
        .create_for_session(OWNER, PROJECT, 0)
        .expect("the owner gets its queue");
    WorkQueueStore::new(data.path())
        .update(PROJECT, OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![WorkItem {
                    node_id: ITEM.to_string(),
                    title: "Wire it".to_string(),
                    files: vec![
                        WorkItemFile {
                            basename: "failure.log".to_string(),
                            size_bytes: 812,
                        },
                        WorkItemFile {
                            basename: "spec.md".to_string(),
                            size_bytes: 2048,
                        },
                    ],
                    ..WorkItem::default()
                }],
                ..WorkQueue::default()
            };
        })
        .expect("the item is filled");
    let request = StartSessionRequest {
        session_token: "token-spawn".to_string(),
        project_id: PROJECT.to_string(),
        work_queue_parent: OWNER.to_string(),
        work_queue_parent_daemon_instance_id: OWNER_HOST.to_string(),
        work_item_id: ITEM.to_string(),
        ..Default::default()
    };
    let handler = Arc::new(QueueOverStore {
        data_dir: data.path().to_path_buf(),
    });
    (
        AQueue {
            _data: data,
            request,
        },
        handler,
    )
}

/// The queue handler the helper asks through: over one store, answering an address it holds
/// exactly as the daemon's handler does. Every other method of the trait is out of scope here.
struct QueueOverStore {
    data_dir: std::path::PathBuf,
}

/// An ended stream: no watcher of a stub queue ever sees a revision.
fn an_ended_stream<T: prost::Message + Default + Send + 'static>(
) -> Result<tddy_work_queue::rpc::ChannelStream<T>, Status> {
    let (_, rx) = tokio::sync::mpsc::unbounded_channel();
    Ok(tddy_work_queue::rpc::ChannelStream::from(rx))
}

#[async_trait]
impl WorkQueueHandler for QueueOverStore {
    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status> {
        let req = request.into_inner();
        let stack = req.stack.expect("an address");
        let record = WorkQueueStore::new(&self.data_dir)
            .read(&stack.project_id, &stack.queue_id)
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(GetWorkQueueResponse {
            summary: None,
            work_queue_json: serde_json::to_string(&record.stack)
                .map_err(|e| Status::internal(e.to_string()))?,
        }))
    }

    async fn add_work_item(
        &self,
        _: Request<tddy_service::proto::work_queue::AddWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::AddWorkItemResponse>, Status> {
        Err(Status::unimplemented("add_work_item"))
    }

    async fn get_pr_status(
        &self,
        _: Request<tddy_service::proto::work_queue::GetPrStatusRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::GetPrStatusResponse>, Status> {
        Err(Status::unimplemented("get_pr_status"))
    }

    async fn query_branch(
        &self,
        _: Request<tddy_service::proto::work_queue::QueryBranchRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::QueryBranchResponse>, Status> {
        Err(Status::unimplemented("query_branch"))
    }

    async fn resolve_work_item_base(
        &self,
        _: Request<tddy_service::proto::work_queue::ResolveWorkItemBaseRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ResolveWorkItemBaseResponse>, Status>
    {
        Err(Status::unimplemented("resolve_work_item_base"))
    }

    async fn link_work_item(
        &self,
        _: Request<tddy_service::proto::work_queue::LinkWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::LinkWorkItemResponse>, Status> {
        Err(Status::unimplemented("link_work_item"))
    }

    async fn repoint_work_item(
        &self,
        _: Request<tddy_service::proto::work_queue::RepointWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::RepointWorkItemResponse>, Status> {
        Err(Status::unimplemented("repoint_work_item"))
    }

    async fn reorder_work_item(
        &self,
        _: Request<tddy_service::proto::work_queue::ReorderWorkItemRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ReorderWorkItemResponse>, Status> {
        Err(Status::unimplemented("reorder_work_item"))
    }

    async fn pull_base_into_branch(
        &self,
        _: Request<tddy_service::proto::work_queue::PullBaseIntoBranchRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::PullBaseIntoBranchResponse>, Status> {
        Err(Status::unimplemented("pull_base_into_branch"))
    }

    async fn list_project_work_queues(
        &self,
        _: Request<tddy_service::proto::work_queue::ListProjectWorkQueuesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::ListProjectWorkQueuesResponse>, Status>
    {
        Err(Status::unimplemented("list_project_work_queues"))
    }

    async fn assign_work_item_pr(
        &self,
        _: Request<tddy_service::proto::work_queue::AssignWorkItemPrRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("assign_work_item_pr"))
    }

    async fn unassign_work_item_pr(
        &self,
        _: Request<tddy_service::proto::work_queue::UnassignWorkItemPrRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("unassign_work_item_pr"))
    }

    async fn link_work_item_session(
        &self,
        _: Request<tddy_service::proto::work_queue::LinkWorkItemSessionRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("link_work_item_session"))
    }

    async fn unlink_work_item_session(
        &self,
        _: Request<tddy_service::proto::work_queue::UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemLinksResponse>, Status> {
        Err(Status::unimplemented("unlink_work_item_session"))
    }

    async fn stream_work_queue_status(
        &self,
        _: Request<tddy_service::proto::work_queue::StreamWorkQueueStatusRequest>,
    ) -> Result<
        Response<
            tddy_work_queue::rpc::ChannelStream<
                tddy_service::proto::work_queue::WorkQueueStatusView,
            >,
        >,
        Status,
    > {
        Ok(Response::new(an_ended_stream()?))
    }

    async fn watch_work_queue(
        &self,
        _: Request<tddy_service::proto::work_queue::WatchWorkQueueRequest>,
    ) -> Result<
        Response<
            tddy_work_queue::rpc::ChannelStream<tddy_service::proto::work_queue::WorkQueueSnapshot>,
        >,
        Status,
    > {
        Ok(Response::new(an_ended_stream()?))
    }
}

#[tokio::test]
async fn a_start_naming_an_item_gains_its_files_as_host_document_references() {
    // Given
    let (queue, handler) = a_queue_with_files();

    // When
    let attachments = work_item_files_for_start(&*handler, OWNER_HOST, &queue.request)
        .await
        .expect("the item's files");

    // Then
    let basenames: Vec<&str> = attachments.iter().map(|a| a.basename.as_str()).collect();
    assert_eq!(basenames, vec!["failure.log", "spec.md"]);
    for attachment in &attachments {
        let Some(Source::HostDocument(host_document)) = attachment.source.as_ref() else {
            panic!("every item file is attached by host-document reference");
        };
        assert_eq!(host_document.scope, HostDocumentScope::WorkQueueFile as i32);
        assert_eq!(host_document.session_id, OWNER);
        assert_eq!(host_document.daemon_instance_id, OWNER_HOST);
    }
}

#[tokio::test]
async fn every_reference_names_the_file_under_its_own_item() {
    // Given
    let (queue, handler) = a_queue_with_files();

    // When
    let attachments = work_item_files_for_start(&*handler, OWNER_HOST, &queue.request)
        .await
        .expect("the item's files");

    // Then
    let relative_paths: Vec<String> = attachments
        .iter()
        .map(|a| match a.source.as_ref().expect("a source") {
            Source::HostDocument(host_document) => host_document.relative_path.clone(),
            Source::Staged(_) => panic!("an item file is never a staged ref"),
        })
        .collect();
    assert_eq!(
        relative_paths,
        vec![format!("{ITEM}/failure.log"), format!("{ITEM}/spec.md")]
    );
}

#[tokio::test]
async fn a_start_that_names_no_item_gains_nothing() {
    // Given
    let (queue, handler) = a_queue_with_files();
    let request = StartSessionRequest {
        work_item_id: String::new(),
        ..queue.request.clone()
    };

    // When
    let attachments = work_item_files_for_start(&*handler, OWNER_HOST, &request)
        .await
        .expect("no files");

    // Then
    assert!(attachments.is_empty(), "got {attachments:?}");
}

#[tokio::test]
async fn a_start_with_no_work_queue_parent_gains_nothing() {
    // Given
    let (queue, handler) = a_queue_with_files();
    let request = StartSessionRequest {
        work_queue_parent: String::new(),
        ..queue.request.clone()
    };

    // When
    let attachments = work_item_files_for_start(&*handler, OWNER_HOST, &request)
        .await
        .expect("no files");

    // Then
    assert!(attachments.is_empty(), "got {attachments:?}");
}

#[tokio::test]
async fn an_item_without_files_gains_nothing() {
    // Given — the item exists but carries no files
    let data = tempfile::tempdir().expect("temp data dir");
    SessionWorkQueues::new(data.path().join("sessions"))
        .create_for_session(OWNER, PROJECT, 0)
        .expect("the owner gets its queue");
    let handler = Arc::new(QueueOverStore {
        data_dir: data.path().to_path_buf(),
    });
    let request = StartSessionRequest {
        session_token: "token-spawn".to_string(),
        project_id: PROJECT.to_string(),
        work_queue_parent: OWNER.to_string(),
        work_item_id: ITEM.to_string(),
        ..Default::default()
    };

    // When
    let attachments = work_item_files_for_start(&*handler, OWNER_HOST, &request)
        .await
        .expect("the queue answers, the item has no files");

    // Then
    assert!(attachments.is_empty(), "got {attachments:?}");
}
