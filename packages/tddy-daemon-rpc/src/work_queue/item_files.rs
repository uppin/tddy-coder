//! `AttachWorkItemFile` and `DetachWorkItemFile` — the files a work item carries.
//!
//! Each acts on the queue its address names, in this host's store (forwarded to the daemon the
//! address names when that is a peer), under the same access as the link calls, and answers the
//! queue after the change — so the queue view's attachment icon and list update live.
//!
//! The bytes live on the queue's owner host, under the owning session's directory
//! (`tddy_changeset::session_work_queue_files`). An attach resolves its source **here** — the
//! owner host is where the RPC lands — and the size cap is this host's configured
//! `max_attachment_bytes` on every arm, local or peer, so a file's fate does not depend on which
//! host the caller is on.

use super::links::{required, LinkCall};
use super::WorkQueueRpcHandler;

use tddy_core::changeset::WorkItemFile;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session_files::ReadHostDocumentRequest;
use tddy_service::proto::types::HostDocumentScope;
use tddy_service::proto::work_queue::{
    AttachWorkItemFileRequest, DetachWorkItemFileRequest, WorkItemLinksResponse,
    WorkQueueFileSource,
};
use tddy_session_files::session_attachment_staging::{delete_staged_attachment, staging_root_for};
use tddy_work_queue::item_ops::item_files::{self, WorkItemFileError};

fn file_refusal(error: WorkItemFileError) -> Status {
    let message = error.to_string();
    match error {
        WorkItemFileError::NoSuchItem(_) => Status::not_found(message),
        WorkItemFileError::Duplicate { .. } => Status::invalid_argument(message),
        WorkItemFileError::TooMany { .. } => Status::invalid_argument(message),
        WorkItemFileError::NoSuchFile { .. } => Status::not_found(message),
    }
}

fn store_refusal(error: tddy_core::session_work_queue_files::WorkQueueFileError) -> Status {
    use tddy_core::session_work_queue_files::WorkQueueFileError as E;
    let message = error.to_string();
    match error {
        E::InvalidSegment { .. } | E::AlreadyExists { .. } => Status::invalid_argument(message),
        E::NotFound { .. } => Status::not_found(message),
        E::Io { .. } => Status::internal(message),
    }
}

impl WorkQueueRpcHandler {
    pub(super) async fn attach_work_item_file_rpc(
        &self,
        request: Request<AttachWorkItemFileRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = match self
            .annotation_call(
                "AttachWorkItemFile",
                &req.session_token,
                req.queue.as_ref(),
                &req,
            )
            .await?
        {
            Ok(call) => call,
            Err(answered) => return Ok(Response::new(answered)),
        };
        let item_id = required(&req.item_id, "item_id")?.to_string();
        let basename = required(&req.basename, "basename")?.to_string();
        let source = req
            .source
            .ok_or_else(|| Status::invalid_argument("source is required"))?;

        let (bytes, size_bytes) = self
            .resolve_work_queue_file_bytes(&call, &req.session_token, &source)
            .await?;
        let cap = self.config.max_attachment_bytes;
        if size_bytes > cap {
            return Err(Status::invalid_argument(format!(
                "'{basename}' is {size_bytes} bytes; a work item file is at most {cap}"
            )));
        }

        let files = self.work_queue_files(&call)?;
        files
            .store(&call.address.queue_id, &item_id, &basename, &bytes)
            .map_err(store_refusal)?;

        let recorded = self.edit_queue(
            &call,
            |queue| {
                item_files::record_attached_file(
                    queue,
                    &item_id,
                    WorkItemFile {
                        basename: basename.clone(),
                        size_bytes,
                    },
                )
            },
            file_refusal,
        );
        if let Err(status) = recorded {
            // The bytes were written for an entry that was never recorded — remove them, so a
            // retry is not blocked and no orphan outlives the refusal.
            let _ = files.remove(&call.address.queue_id, &item_id, &basename);
            return Err(status);
        }

        // The staged copy is consumed: deleted where this host can reach it. A staged ref naming a
        // peer is fetched from that host and its copy there is left to the staging root's own
        // restart-cleared lifecycle — the same fate a `StartSession`'s consumed batch has today
        // (docs/dev/todo/2026-08-01-session-attach-ui-follow-ups.md).
        if let Some(staged) = staged_of(&source) {
            let local = staged.daemon_instance_id.trim().is_empty()
                || staged.daemon_instance_id.trim() == self.local_instance_id();
            if local {
                let root = staging_root_for(&call.os_user, &self.staging_base_dir);
                let _ = delete_staged_attachment(&root, &staged.staging_id, &staged.file_name);
            }
        }

        recorded
    }

    pub(super) async fn detach_work_item_file_rpc(
        &self,
        request: Request<DetachWorkItemFileRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = match self
            .annotation_call(
                "DetachWorkItemFile",
                &req.session_token,
                req.queue.as_ref(),
                &req,
            )
            .await?
        {
            Ok(call) => call,
            Err(answered) => return Ok(Response::new(answered)),
        };
        let item_id = required(&req.item_id, "item_id")?;
        let basename = required(&req.basename, "basename")?;

        // The bytes go first: a refusal here leaves the queue exactly as it was, and an entry whose
        // bytes vanished under it cannot arise from an attach, which writes bytes only before its
        // entry is recorded.
        self.work_queue_files(&call)?
            .remove(&call.address.queue_id, item_id, basename)
            .map_err(store_refusal)?;
        self.edit_queue(
            &call,
            |queue| item_files::record_detached_file(queue, item_id, basename),
            file_refusal,
        )
    }

    fn work_queue_files(
        &self,
        call: &LinkCall,
    ) -> Result<tddy_core::session_work_queue_files::SessionWorkQueueFiles, Status> {
        // `sessions_base` is the daemon's data root; the files store takes the sessions directory
        // under it, exactly as `SessionWorkQueues` does.
        let sessions_dir = self.sessions_base(&call.os_user)?.join("sessions");
        Ok(tddy_core::session_work_queue_files::SessionWorkQueueFiles::new(sessions_dir))
    }

    /// The bytes a `WorkQueueFileSource` names, read on this host — the owner host, wherever the
    /// caller sits. A staged ref naming a peer is fetched from it over the streaming host-document
    /// read, held to this host's `max_attachment_bytes` exactly as the local read is.
    async fn resolve_work_queue_file_bytes(
        &self,
        call: &LinkCall,
        session_token: &str,
        source: &WorkQueueFileSource,
    ) -> Result<(Vec<u8>, u64), Status> {
        use tddy_service::proto::work_queue::work_queue_file_source::Origin;

        match source.origin.as_ref() {
            Some(Origin::Staged(staged)) => {
                let relative_path = format!("{}/{}", staged.staging_id, staged.file_name);
                self.read_host_document_bytes(
                    call,
                    session_token,
                    ReadRef {
                        scope: HostDocumentScope::StagedAttachment,
                        ref_daemon: &staged.daemon_instance_id,
                        session_id: "",
                        project_id: "",
                        relative_path: &relative_path,
                    },
                )
                .await
            }
            Some(Origin::ArtifactRelativePath(relative_path)) => {
                // The queue's id is its owning session's id, so this arm reads the owner's own
                // artifacts and nothing else's.
                self.read_host_document_bytes(
                    call,
                    session_token,
                    ReadRef {
                        scope: HostDocumentScope::SessionArtifact,
                        ref_daemon: "",
                        session_id: &call.address.queue_id,
                        project_id: "",
                        relative_path,
                    },
                )
                .await
            }
            Some(Origin::RepoFilePath(relative_path)) => {
                self.read_host_document_bytes(
                    call,
                    session_token,
                    ReadRef {
                        scope: HostDocumentScope::ProjectRepo,
                        ref_daemon: "",
                        session_id: "",
                        project_id: &call.address.project_id,
                        relative_path,
                    },
                )
                .await
            }
            None => Err(Status::invalid_argument(
                "source must name a staged upload, a session artifact or a repo file",
            )),
        }
    }

    /// The bytes of one host document: locally when the ref names this host, else streamed from the
    /// peer that owns it — the same routing the start-session materializer runs, held to this
    /// host's configured cap on both paths.
    async fn read_host_document_bytes(
        &self,
        call: &LinkCall,
        session_token: &str,
        read: ReadRef<'_>,
    ) -> Result<(Vec<u8>, u64), Status> {
        let scope = read.scope;
        let session_id = read.session_id;
        let project_id = read.project_id;
        let relative_path = read.relative_path;
        let ref_daemon = read.ref_daemon.trim();
        let local = ref_daemon.is_empty() || ref_daemon == self.local_instance_id();
        if local {
            let resolved = tddy_session_files::host_documents::resolve_host_document(
                &call.os_user,
                &self.tddy_data_dir,
                &self.staging_base_dir,
                scope,
                session_id,
                project_id,
                relative_path,
            )?;
            let cap = self.config.max_attachment_bytes;
            if resolved.byte_size > cap {
                return Err(Status::invalid_argument(format!(
                    "the file is {} bytes; a work item file is at most {cap}",
                    resolved.byte_size
                )));
            }
            use tddy_fs::Fs;
            let bytes = tddy_fs::DiskFs
                .read(&resolved.path)
                .map_err(|e| Status::internal(format!("failed to read the file: {e}")))?;
            return Ok((bytes, resolved.byte_size));
        }

        let slot = self
            .peer_routing
            .common_room_slot("StreamReadHostDocument")?;
        let read_req = ReadHostDocumentRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: ref_daemon.to_string(),
            scope: scope.into(),
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            relative_path: relative_path.to_string(),
        };
        let mut frames = tddy_daemon_kernel::peer_calls::forward_stream_read_host_document(
            slot, ref_daemon, &read_req,
        )
        .await?;
        let cap = self.config.max_attachment_bytes;
        let mut data: Vec<u8> = Vec::new();
        let mut total: Option<u64> = None;
        while let Some(frame) = frames.recv().await {
            let frame = frame?;
            if total.is_none() {
                total = Some(frame.total_byte_size);
            }
            data.extend_from_slice(&frame.data);
            if data.len() as u64 > cap {
                return Err(Status::invalid_argument(format!(
                    "the file is at least {} bytes; a work item file is at most {cap}",
                    data.len()
                )));
            }
        }
        let total = total.unwrap_or(0);
        Ok((data, total))
    }
}

/// One host-document read an attach resolves: the scope, the host the bytes live on, and the
/// coordinates the scope resolves under.
struct ReadRef<'a> {
    scope: HostDocumentScope,
    ref_daemon: &'a str,
    session_id: &'a str,
    project_id: &'a str,
    relative_path: &'a str,
}

fn staged_of(
    source: &WorkQueueFileSource,
) -> Option<tddy_service::proto::session::StagedAttachmentRef> {
    use tddy_service::proto::work_queue::work_queue_file_source::Origin;
    match source.origin.as_ref() {
        Some(Origin::Staged(staged)) => Some(staged.clone()),
        _ => None,
    }
}
