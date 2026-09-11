//! The `session_files.SessionFilesService` implementation, and the entry the daemon's wiring layer
//! registers.
//!
//! Every one of the thirteen methods reads or writes a file under a path the *serving* host resolves
//! from the caller's session token. None of them trusts a path, an OS user or an allow-list row the
//! request supplied, which is why the ports below are resolvers rather than values: a caller holding
//! a valid token must not be able to name a root it does not own.
//!
//! # What is deliberately not here
//!
//! `daemon_instance_id` routing is **not** implemented in this crate. Forwarding a call to the peer
//! that holds the bytes needs the common room slot, the peer registry and the per-method
//! `forward_*_via_livekit` clients, all of which are the daemon's transport layer — a session-file
//! reader that reached for them would be back inside the module this crate was extracted from.
//! The daemon wraps this entry in its own routing layer
//! (`tddy-daemon`'s `PeerRoutedSessionFiles`), which is where forwarding lives.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session_files::{
    ContextFileBatchChunk, ContextFileChunk, ContextManifestEntry, ContextManifestRequest,
    DeleteSessionUploadRequest, DeleteSessionUploadResponse, DeleteStagedAttachmentRequest,
    DeleteStagedAttachmentResponse, HostDocumentChunk, ListSessionUploadsRequest,
    ListSessionUploadsResponse, ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    ListStagedAttachmentsRequest, ListStagedAttachmentsResponse, ReadContextFileBatchRequest,
    ReadContextFileRequest, ReadHostDocumentRequest, ReadHostDocumentResponse,
    ReadSessionWorkflowFileRequest, ReadSessionWorkflowFileResponse, SessionUploadEntry,
    StagedAttachmentEntry, UploadSessionFileChunkRequest, UploadSessionFileChunkResponse,
    UploadStagedAttachmentChunkRequest, UploadStagedAttachmentChunkResponse, WorkflowFileEntry,
};
use tddy_service::SessionFilesServiceServer;
use tddy_worktree_service::stream::MpscResultStream;

use crate::HostDocumentScope;

/// Resolves a session token to the OS user that owns it, or to the refusal.
///
/// A `Result` rather than the kernel's `Option`-returning [`tddy_daemon_kernel::SessionUserResolver`]
/// because the two refusals here send a caller to different remedies and must not collapse into
/// one: an unknown or expired token is `UNAUTHENTICATED` and the caller re-authenticates, while a
/// known GitHub user with no OS-user mapping is `PERMISSION_DENIED` and only an operator can fix
/// it. The mapping table is the daemon's config, so the distinction can only be drawn where the
/// closure is built.
pub type OsUserResolver = Arc<dyn Fn(&str) -> Result<String, Status> + Send + Sync>;

/// The checkout a session's agent guidance is read from, and the allow-list row it is served under.
///
/// Both come from what the serving daemon *persisted* about the session rather than from the
/// request: authorization on this path is per OS user, so a caller holding a valid token for one of
/// its sessions can name any other session of the same user — and if the request's `agent` field
/// decided the row, it could also name any row, and be served that checkout's `.claude/**`,
/// `.cursor/**` and `.mcp.json`. Those are the files that routinely carry API tokens in MCP `env`
/// blocks.
pub struct SessionContextScope {
    pub worktree_root: PathBuf,
    pub globs: &'static [&'static str],
}

/// Resolves one request to the checkout and allow-list row it may read.
///
/// A port rather than a lookup this crate performs, because resolving a session to its checkout
/// reads the sandbox registry as well as the session metadata, and both stay in `tddy-daemon`. What
/// this crate owns is the reading and the refusals — [`crate::context_files`] — and those need only
/// the root and the row.
pub trait SessionContextScopes: Send + Sync {
    fn scope_for(
        &self,
        session_token: &str,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<SessionContextScope, Status>;
}

/// Everything the thirteen handlers need from the host they run on.
///
/// A struct rather than seven positional parameters: they are all wiring, and two of them are
/// `PathBuf`s that a call site could silently swap for each other — a data dir passed as a staging
/// base would resolve every scope root one directory off and refuse everything as absent.
pub struct SessionFilesPorts {
    /// Session token to the OS user that owns it. The staging root and every host-document scope
    /// root is resolved under this user, never under the referencing client's.
    pub os_users: OsUserResolver,
    /// The daemon's data dir. `projects/` under it answers the `PROJECT_REPO` scope, and
    /// `user_paths::sessions_base_for_user` derives a user's sessions base from it.
    pub tddy_data_dir: PathBuf,
    /// The restart-cleared pre-session staging base
    /// ([`crate::session_attachment_staging::default_staging_base_dir`]).
    pub staging_base_dir: PathBuf,
    /// This host's configured `max_attachment_bytes` — the cap a context read and a streamed host
    /// document are refused by *before* their first frame, rather than truncated at.
    pub max_attachment_bytes: u64,
    /// This daemon's instance id, stamped on every staged-attachment entry it answers with so a
    /// `StagedAttachmentRef` names the host holding the bytes.
    pub daemon_instance_id: String,
    /// Where a context read is served from.
    pub context_scopes: Arc<dyn SessionContextScopes>,
    /// How long one blocking context read may take before the call is refused — this host's
    /// `spawn_worker_request_timeout` (`spawn_worker_request_timeout_secs`), which is the key the
    /// refusal names so an operator who hits it knows what to raise.
    ///
    /// Supplied by the daemon rather than chosen here, like every other field above: the budget is
    /// an operator's tuning of the host doing the reading, not a property of this subsystem. It is
    /// the same budget the daemon gives its other filesystem work, so a context read and a worktree
    /// build on one host cannot disagree about how long that host is allowed to take.
    pub context_read_deadline: Duration,
}

/// The `session_files.SessionFilesService` implementation.
pub struct SessionFilesServiceImpl {
    ports: SessionFilesPorts,
}

impl SessionFilesServiceImpl {
    #[must_use]
    pub fn new(ports: SessionFilesPorts) -> Self {
        Self { ports }
    }

    /// The OS user a token belongs to, or why it may not be served.
    fn os_user(&self, session_token: &str) -> Result<String, Status> {
        (self.ports.os_users)(session_token)
    }

    /// The base directory a token's owner keeps its sessions under.
    ///
    /// Derived from the OS user rather than injected as a second resolver: a session directory and
    /// a staging root that disagreed about which user a token belongs to would be a confused
    /// deputy, and one resolver cannot disagree with itself.
    fn sessions_base(&self, session_token: &str) -> Result<PathBuf, Status> {
        let os_user = self.os_user(session_token)?;
        tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.ports.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))
    }

    /// The session directory a request addresses, with its id validated as one path segment first —
    /// a session id is untrusted client input that becomes a path component.
    fn session_dir(&self, session_token: &str, session_id: &str) -> Result<PathBuf, Status> {
        let sessions_base = self.sessions_base(session_token)?;
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(unified_session_dir_path(&sessions_base, session_id))
    }

    /// The caller's own staging root, under the OS user its token resolves to.
    fn staging_root(&self, session_token: &str) -> Result<PathBuf, Status> {
        let os_user = self.os_user(session_token)?;
        Ok(crate::session_attachment_staging::staging_root_for(
            &os_user,
            &self.ports.staging_base_dir,
        ))
    }

    /// One staged file as the wire describes it, stamped with this host's instance id.
    fn staged_entry(
        &self,
        staging_id: String,
        file_name: String,
        host_path: &Path,
    ) -> StagedAttachmentEntry {
        let metadata = std::fs::metadata(host_path).ok();
        StagedAttachmentEntry {
            daemon_instance_id: self.ports.daemon_instance_id.clone(),
            staging_id,
            file_name,
            host_path: host_path.to_string_lossy().into_owned(),
            size_bytes: metadata.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
            staged_at_ms: crate::session_attachment_staging::staged_at_ms(host_path),
        }
    }

    /// A host document resolved under the caller's OS user, with every scope, containment and
    /// completeness guard applied and no bytes read.
    fn resolve_document(
        &self,
        req: &ReadHostDocumentRequest,
    ) -> Result<crate::host_documents::ResolvedHostDocument, Status> {
        let os_user = self.os_user(&req.session_token)?;
        // An unrecognised scope number becomes `Unspecified`, which `resolve_host_document`
        // refuses. Defaulting it to a real scope would read a file from the wrong root and answer
        // as if that were what was asked for.
        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        crate::host_documents::resolve_host_document(
            &os_user,
            &self.ports.tddy_data_dir,
            &self.ports.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )
    }
}

/// Send every frame into a fresh channel and hand back the stream reading it.
///
/// The frames are already in memory — the reader that produced them is the thing that applied the
/// cap, so an over-cap read was refused before any frame existed and nothing here can be a partial
/// answer. A dropped receiver stops the loop rather than filling a channel nobody reads.
fn streamed<T>(frames: Vec<T>) -> Response<MpscResultStream<T>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<T, Status>>();
    for frame in frames {
        if tx.send(Ok(frame)).is_err() {
            break;
        }
    }
    Response::new(MpscResultStream::from(rx))
}

/// Run one blocking context read off the async runtime, bounded by `deadline`.
///
/// Both halves matter and neither is optional. The read is `spawn_blocking` because hashing or
/// reading every allow-listed path is filesystem work that would otherwise hold a runtime worker;
/// and it is bounded because that filesystem can genuinely stall — the caller is usually a split
/// session's agent host fetching its guidance from the host that holds the codebase. Unbounded, a
/// stalled read leaves the RPC waiting for exactly as long as the read does, with nothing for the
/// caller to retry or report; bounded, it answers `DEADLINE_EXCEEDED`.
///
/// The refusal names `spawn_worker_request_timeout_secs` because the remedy is to raise that key,
/// and this message is the only place an operator is told so.
async fn read_within_deadline<T, Read>(
    rpc_name: &str,
    deadline: Duration,
    read: Read,
) -> Result<T, Status>
where
    Read: FnOnce() -> Result<T, Status> + Send + 'static,
    T: Send + 'static,
{
    let join = tokio::task::spawn_blocking(read);
    match tokio::time::timeout(deadline, join).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(refusal))) => Err(refusal),
        Ok(Err(join_error)) => Err(Status::internal(join_error.to_string())),
        Err(_elapsed) => {
            let secs = deadline.as_secs();
            Err(Status::deadline_exceeded(format!(
                "{rpc_name}: timed out after {secs}s (spawn_worker_request_timeout_secs)"
            )))
        }
    }
}

#[async_trait]
impl tddy_service::proto::session_files::SessionFilesService for SessionFilesServiceImpl {
    async fn list_session_workflow_files(
        &self,
        request: Request<ListSessionWorkflowFilesRequest>,
    ) -> Result<Response<ListSessionWorkflowFilesResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let basenames =
            crate::session_workflow_files::list_allowlisted_workflow_basenames(&session_dir)?;
        Ok(Response::new(ListSessionWorkflowFilesResponse {
            files: basenames
                .into_iter()
                .map(|basename| WorkflowFileEntry { basename })
                .collect(),
        }))
    }

    async fn read_session_workflow_file(
        &self,
        request: Request<ReadSessionWorkflowFileRequest>,
    ) -> Result<Response<ReadSessionWorkflowFileResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;
        let content_utf8 = crate::session_workflow_files::read_allowlisted_workflow_file_utf8(
            &session_dir,
            &req.basename,
        )?;
        Ok(Response::new(ReadSessionWorkflowFileResponse {
            content_utf8,
        }))
    }

    type StreamContextManifestStream = MpscResultStream<ContextManifestEntry>;

    /// Hashing every allow-listed file is filesystem work, so it runs off the async runtime — and it
    /// runs *before* the stream exists, because a refusal has to reach the caller as the call's own
    /// error rather than as a mid-stream item it would have to tell apart from a transport failure.
    async fn stream_context_manifest(
        &self,
        request: Request<ContextManifestRequest>,
    ) -> Result<Response<Self::StreamContextManifestStream>, Status> {
        let req = request.into_inner();
        let scope =
            self.ports
                .context_scopes
                .scope_for(&req.session_token, &req.session_id, &req.agent)?;
        let max_bytes = self.ports.max_attachment_bytes;
        let entries = read_within_deadline(
            "StreamContextManifest",
            self.ports.context_read_deadline,
            move || {
                crate::context_files::context_manifest(&scope.worktree_root, scope.globs, max_bytes)
            },
        )
        .await?;
        Ok(streamed(entries))
    }

    type StreamReadContextFileStream = MpscResultStream<ContextFileChunk>;

    async fn stream_read_context_file(
        &self,
        request: Request<ReadContextFileRequest>,
    ) -> Result<Response<Self::StreamReadContextFileStream>, Status> {
        let req = request.into_inner();
        let scope =
            self.ports
                .context_scopes
                .scope_for(&req.session_token, &req.session_id, &req.agent)?;
        let max_bytes = self.ports.max_attachment_bytes;
        let rel_path = req.rel_path;
        let bytes = read_within_deadline(
            "StreamReadContextFile",
            self.ports.context_read_deadline,
            move || {
                crate::context_files::read_context_file_bytes(
                    &scope.worktree_root,
                    &rel_path,
                    scope.globs,
                    max_bytes,
                )
            },
        )
        .await?;
        Ok(streamed(crate::context_files::context_file_frames(&bytes)))
    }

    type StreamReadContextFileBatchStream = MpscResultStream<ContextFileBatchChunk>;

    /// One refusal fails the whole batch, before any bytes are read
    /// ([`crate::context_files::read_context_files_bytes`]): serving what it can would leave the
    /// caller unable to tell "the project does not ship that file" from "this host would not serve
    /// it", and the setup sync this exists for must fail loudly rather than start an agent against
    /// guidance with a hole in it.
    async fn stream_read_context_file_batch(
        &self,
        request: Request<ReadContextFileBatchRequest>,
    ) -> Result<Response<Self::StreamReadContextFileBatchStream>, Status> {
        let req = request.into_inner();
        let scope =
            self.ports
                .context_scopes
                .scope_for(&req.session_token, &req.session_id, &req.agent)?;
        let max_bytes = self.ports.max_attachment_bytes;
        let rel_paths = req.rel_paths;
        let files = read_within_deadline(
            "StreamReadContextFileBatch",
            self.ports.context_read_deadline,
            move || {
                crate::context_files::read_context_files_bytes(
                    &scope.worktree_root,
                    &rel_paths,
                    scope.globs,
                    max_bytes,
                )
            },
        )
        .await?;
        Ok(streamed(crate::context_files::context_file_batch_frames(
            &files,
        )))
    }

    async fn upload_session_file_chunk(
        &self,
        request: Request<UploadSessionFileChunkRequest>,
    ) -> Result<Response<UploadSessionFileChunkResponse>, Status> {
        let req = request.into_inner();
        // Authenticated before any filesystem access, so an invalid token never creates a directory.
        let sessions_base = self.sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let host_path = crate::session_file_upload::write_upload_chunk(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;
        Ok(Response::new(UploadSessionFileChunkResponse {
            host_path: host_path
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }))
    }

    async fn list_session_uploads(
        &self,
        request: Request<ListSessionUploadsRequest>,
    ) -> Result<Response<ListSessionUploadsResponse>, Status> {
        let req = request.into_inner();
        let sessions_base = self.sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let uploads = crate::session_uploads::list_uploads(&sessions_base, &req.session_id)?;
        Ok(Response::new(ListSessionUploadsResponse {
            uploads: uploads
                .into_iter()
                .map(|u| SessionUploadEntry {
                    upload_id: u.upload_id,
                    file_name: u.file_name,
                    host_path: u.host_path.to_string_lossy().into_owned(),
                    size_bytes: u.size_bytes,
                    uploaded_at_ms: u.uploaded_at_ms,
                })
                .collect(),
        }))
    }

    async fn delete_session_upload(
        &self,
        request: Request<DeleteSessionUploadRequest>,
    ) -> Result<Response<DeleteSessionUploadResponse>, Status> {
        let req = request.into_inner();
        let sessions_base = self.sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        crate::session_uploads::delete_upload(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteSessionUploadResponse {}))
    }

    async fn upload_staged_attachment_chunk(
        &self,
        request: Request<UploadStagedAttachmentChunkRequest>,
    ) -> Result<Response<UploadStagedAttachmentChunkResponse>, Status> {
        let req = request.into_inner();
        let staging_root = self.staging_root(&req.session_token)?;
        let host_path = crate::session_attachment_staging::write_staged_chunk(
            &staging_root,
            &req.staging_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;
        // Populated only on the final chunk: an entry on a mid-upload chunk would describe a file
        // that is not yet whole, and a `StagedAttachmentRef` built from it would name one.
        let entry = host_path.map(|path| self.staged_entry(req.staging_id, req.file_name, &path));
        Ok(Response::new(UploadStagedAttachmentChunkResponse { entry }))
    }

    async fn list_staged_attachments(
        &self,
        request: Request<ListStagedAttachmentsRequest>,
    ) -> Result<Response<ListStagedAttachmentsResponse>, Status> {
        let req = request.into_inner();
        let staging_root = self.staging_root(&req.session_token)?;
        let files = crate::session_attachment_staging::list_staged_attachments(
            &staging_root,
            &req.staging_id,
        )?;
        Ok(Response::new(ListStagedAttachmentsResponse {
            attachments: files
                .into_iter()
                .map(|f| StagedAttachmentEntry {
                    daemon_instance_id: self.ports.daemon_instance_id.clone(),
                    staging_id: f.staging_id,
                    file_name: f.file_name,
                    host_path: f.host_path.to_string_lossy().into_owned(),
                    size_bytes: f.size_bytes,
                    staged_at_ms: f.staged_at_ms,
                })
                .collect(),
        }))
    }

    async fn delete_staged_attachment(
        &self,
        request: Request<DeleteStagedAttachmentRequest>,
    ) -> Result<Response<DeleteStagedAttachmentResponse>, Status> {
        let req = request.into_inner();
        let staging_root = self.staging_root(&req.session_token)?;
        crate::session_attachment_staging::delete_staged_attachment(
            &staging_root,
            &req.staging_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteStagedAttachmentResponse {}))
    }

    async fn read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<ReadHostDocumentResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.os_user(&req.session_token)?;
        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        let doc = crate::host_documents::read_host_document_bytes(
            &os_user,
            &self.ports.tddy_data_dir,
            &self.ports.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )?;
        Ok(Response::new(ReadHostDocumentResponse {
            data: doc.data,
            byte_size: doc.byte_size,
        }))
    }

    type StreamReadHostDocumentStream = MpscResultStream<HostDocumentChunk>;

    /// The cap is checked before the first frame, so an over-cap document is refused rather than
    /// streamed and cut short: a consumer cannot tell a truncated document from a whole one once
    /// the frames have started.
    async fn stream_read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<Self::StreamReadHostDocumentStream>, Status> {
        let req = request.into_inner();
        let resolved = self.resolve_document(&req)?;

        let max_bytes = self.ports.max_attachment_bytes;
        if resolved.byte_size > max_bytes {
            return Err(Status::invalid_argument(format!(
                "host document exceeds this host's maximum attachment size of {max_bytes} bytes"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<HostDocumentChunk, Status>>();
        tokio::task::spawn_blocking(move || {
            crate::host_documents::stream_document_frames(
                &resolved.path,
                resolved.byte_size,
                |data, total_byte_size| HostDocumentChunk {
                    data,
                    total_byte_size,
                },
                &tx,
            );
        });
        Ok(Response::new(MpscResultStream::from(rx)))
    }
}

/// The `session_files.SessionFilesService` entry the daemon's wiring layer registers.
///
/// `#unbundle` node 6 moved these four families out of `connection.ConnectionService` and into the
/// crate that now owns all ten of their modules. The ports stay injected because each one is
/// *wiring*: which OS user a token maps to, where this host keeps its data and its staging area,
/// what it caps an attachment at, and which checkout a session's guidance is read from are the
/// daemon's answers, not this subsystem's behaviour.
#[must_use]
pub fn build_session_files_entry(ports: SessionFilesPorts) -> tddy_rpc::ServiceEntry {
    let server = SessionFilesServiceServer::new(SessionFilesServiceImpl::new(ports));
    tddy_rpc::ServiceEntry {
        name: tddy_service::SESSION_FILES_SERVICE,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
