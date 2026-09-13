//! What this daemon hands `tddy-session-files` so that crate can serve
//! `session_files.SessionFilesService`, and the routing it keeps for itself.
//!
//! The thirteen session-file methods are `tddy-session-files`'; what stays here is the seven
//! answers only a daemon has — which OS user a session token belongs to, where this host keeps its
//! data and its staging area, what it caps an attachment at, which instance id it stamps on a
//! staged entry, which checkout a session's agent guidance is read from, and how long a read of
//! that checkout may take.
//!
//! Eight of the thirteen also **route**: a request naming another daemon is served by that daemon,
//! not here. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-files` may reach for — its module header says so
//! — which is why [`PeerRoutedSessionFiles`] wraps the crate's implementation rather than the crate
//! growing a transport. Every decision below is made by a [`ConnectionServiceImpl`] method rather
//! than re-derived here, so a request that arrives on the wire and one this daemon makes for itself
//! cannot disagree about which host holds a file.

use std::sync::Arc;

use async_trait::async_trait;
use livekit::prelude::Room;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::connection::ExecuteToolRequest;
use tddy_service::proto::session_files::{
    ContextFileBatchChunk, ContextFileChunk, ContextManifestEntry, ContextManifestRequest,
    DeleteSessionUploadRequest, DeleteSessionUploadResponse, DeleteStagedAttachmentRequest,
    DeleteStagedAttachmentResponse, HostDocumentChunk, ListSessionUploadsRequest,
    ListSessionUploadsResponse, ListSessionWorkflowFilesRequest, ListSessionWorkflowFilesResponse,
    ListStagedAttachmentsRequest, ListStagedAttachmentsResponse, ReadContextFileBatchRequest,
    ReadContextFileRequest, ReadHostDocumentRequest, ReadHostDocumentResponse,
    ReadSessionWorkflowFileRequest, ReadSessionWorkflowFileResponse, SessionFilesService,
    UploadSessionFileChunkRequest, UploadSessionFileChunkResponse,
    UploadStagedAttachmentChunkRequest, UploadStagedAttachmentChunkResponse,
};
use tddy_session_files::service::{SessionContextScope, SessionContextScopes};
use tddy_session_files::{SessionFilesPorts, SessionFilesServiceImpl};
use tddy_worktree_service::stream::MpscResultStream;

use super::ConnectionServiceImpl;
use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};

/// The coordinate a forward is addressed at on the peer. A forwarded call has to land on the same
/// method of the same service there, which is where the peer declares these eight — so the name is
/// the one `tddy-service` publishes, the same value `tddy-session-files` serves under and
/// `tddy-daemon-livekit`'s forwarders address.
const SESSION_FILES_SERVICE: &str = tddy_service::SESSION_FILES_SERVICE;

/// The label the daemon's exec-tool authorization logs name a context read by.
///
/// The three context RPCs pass their own proto method name there; this port serves all three
/// behind one [`SessionContextScopes::scope_for`], which is not told which. Naming the surface
/// rather than picking one of the three keeps the log honest — the refusals themselves carry no
/// method name in either case.
const CONTEXT_SCOPE_CALLER: &str = "SessionFilesService context read";

/// The common-room handle a forward is sent over.
type CommonRoomSlot = Arc<tokio::sync::RwLock<Option<Arc<Room>>>>;

impl ConnectionServiceImpl {
    /// The `session_files.SessionFilesService` entry this daemon registers.
    ///
    /// Built from the *same* config, resolvers and staging base this daemon's `StartSession`
    /// materializes attachments from, so a batch staged over this coordinate is the batch a start
    /// addressed to `connection.ConnectionService` consumes — a second staging base here would mean
    /// an upload was invisible to the session it was staged for.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn session_files_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: SESSION_FILES_SERVICE,
            service: Arc::new(tddy_service::SessionFilesServiceServer::new(
                self.session_files_service(),
            )) as Arc<dyn tddy_rpc::RpcService>,
        }
    }

    /// Every coordinate a **session room** serves.
    ///
    /// A session room is reached by the agents inside it, and what they may ask for is whatever
    /// this daemon declares. `#unbundle` node 6 moved the session-file and terminal families onto
    /// their own services and node 7 the roster, conversation and activity ones, so a room serving
    /// `connection.ConnectionService` alone would have quietly stopped answering a question it had
    /// always answered — an in-room agent's `ReadHostDocument` would come back "unknown service"
    /// rather than with the document, and an in-jail `StreamSessionAgents` addressed at the new
    /// coordinate would find no roster at all.
    #[must_use]
    pub(crate) fn session_room_roster(self: &Arc<Self>) -> tddy_rpc::MultiRpcService {
        tddy_rpc::MultiRpcService::new(vec![
            self.session_files_entry(),
            self.session_agents_entry(),
            self.activity_entry(),
            self.terminal_session_entry(),
            tddy_rpc::ServiceEntry {
                name: "connection.ConnectionService",
                service: Arc::new(tddy_service::ConnectionServiceServer::from_arc(Arc::clone(
                    self,
                ))) as Arc<dyn tddy_rpc::RpcService>,
            },
        ])
    }

    /// This daemon's session-file surface: the crate's thirteen handlers, with the eight routed
    /// ones answered by the daemon that holds the bytes.
    ///
    /// Public because it *is* the surface — the entry above is this served over a transport, and a
    /// caller inside the daemon (or an acceptance test) that holds the service rather than the
    /// entry must make the same routing decision a request on the wire does. Before `#unbundle`
    /// node 6 these were `ConnectionServiceImpl`'s own methods and routed for every caller; a
    /// surface that routed only for wire callers would serve a request naming another host out of
    /// this host's own directories.
    #[must_use]
    pub fn session_files_service(self: &Arc<Self>) -> PeerRoutedSessionFiles {
        PeerRoutedSessionFiles {
            connection: Arc::clone(self),
            local: SessionFilesServiceImpl::new(self.session_files_ports()),
        }
    }

    /// The seven host answers the thirteen handlers need, each read off this daemon.
    fn session_files_ports(self: &Arc<Self>) -> SessionFilesPorts {
        let for_tokens = Arc::clone(self);
        SessionFilesPorts {
            // `resolve_os_user` already draws the crate's two distinct refusals — an unverifiable
            // token is `UNAUTHENTICATED`, a GitHub user with no `users[]` row is
            // `PERMISSION_DENIED` — from this daemon's own config.
            os_users: Arc::new(move |session_token: &str| {
                for_tokens.resolve_os_user(session_token)
            }),
            tddy_data_dir: self.tddy_data_dir.clone(),
            staging_base_dir: self.staging_base_dir.clone(),
            max_attachment_bytes: self.config.max_attachment_bytes,
            daemon_instance_id: local_instance_id_for_config(&self.config),
            context_scopes: Arc::new(SessionsOfThisDaemon {
                connection: Arc::clone(self),
            }),
            // The same budget this daemon gives its other filesystem work, so a context read and a
            // worktree build on this host cannot disagree about how long it may take — and the
            // budget an operator already tunes rather than a second key to discover.
            context_read_deadline: self.config.spawn_worker_request_timeout(),
        }
    }
}

/// Where a context read is served from: the checkout this daemon recorded for the session, under
/// the allow-list row the session's own persisted `session_type` names.
///
/// Both answers come from [`ConnectionServiceImpl`] rather than being re-derived here, because both
/// are the gate: `resolve_exec_tool_worktree` authenticates the caller and refuses a session id
/// that is not one path segment, and `context_globs_for_session` serves the session's row rather
/// than the `agent` the request claimed. A second derivation of either is a second answer to "may
/// this caller read this file".
struct SessionsOfThisDaemon {
    connection: Arc<ConnectionServiceImpl>,
}

impl SessionContextScopes for SessionsOfThisDaemon {
    fn scope_for(
        &self,
        session_token: &str,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<SessionContextScope, Status> {
        self.connection
            .session_context_scope(session_token, session_id, requested_agent)
    }
}

impl ConnectionServiceImpl {
    /// The checkout and allow-list one context read is served under, resolved once.
    ///
    /// Inherent rather than only behind [`SessionContextScopes`] because this daemon reads its own
    /// context too: a split start fetches the codebase host's guidance, and when that host is this
    /// one the read must be gated by the same two calls a wire caller's is
    /// (`svc_split_context_from_codebase_host`). A second derivation would be a second answer to
    /// "may this caller read this file".
    pub(crate) fn session_context_scope(
        &self,
        session_token: &str,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<SessionContextScope, Status> {
        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
                tool_name: CONTEXT_SCOPE_CALLER.to_string(),
                args_json: String::new(),
                daemon_instance_id: String::new(),
            })?;
        let globs = self.context_globs_for_session(
            CONTEXT_SCOPE_CALLER,
            &sessions_base,
            session_id,
            requested_agent,
        )?;
        Ok(SessionContextScope {
            worktree_root,
            globs,
        })
    }
}

/// The crate's thirteen handlers, with the eight routed ones answered by the daemon that holds the
/// bytes.
///
/// The wrapper is *this* side of the boundary on purpose: `HostDocumentPicker` browses another
/// host by `browsedDaemonInstanceId`, and a coordinate that served every request locally would
/// answer such a browse with this host's files — an empty or refused list that a client cannot
/// tell from a genuinely empty directory.
///
/// It implements the generated service trait rather than wrapping the entry's encoded
/// [`tddy_rpc::RpcService`], so the fork sits in front of the *handler* — the layer it was in
/// before `#unbundle` node 6 moved these methods off `connection.ConnectionService`. Routing a
/// step later, at the transport, would leave every in-process caller of the surface serving a
/// request that names another host out of this host's own directories, and would decode each
/// routed request a second time to find the id it routes on.
pub struct PeerRoutedSessionFiles {
    connection: Arc<ConnectionServiceImpl>,
    /// The `tddy-session-files` implementation, which serves every request this daemon keeps.
    local: SessionFilesServiceImpl,
}

impl PeerRoutedSessionFiles {
    /// The peer one of the five staging and host-document calls is addressed at, with the room to
    /// forward it over — or `None` when the call is this daemon's own to serve.
    ///
    /// The caller is authenticated **first**, which is the order the `connection.ConnectionService`
    /// handlers these five moved off used: an anonymous request must not be able to drive an
    /// outbound forward and hold a pending-call slot on two hosts for the forward's whole deadline.
    /// The route itself is [`ConnectionServiceImpl::classify_daemon_route`], refusals and all — the
    /// same decision every other routed RPC on this daemon makes.
    fn forward_target(
        &self,
        rpc_name: &str,
        session_token: &str,
        daemon_instance_id: &str,
    ) -> Result<Option<(&CommonRoomSlot, String)>, Status> {
        self.connection.resolve_os_user(session_token)?;
        let PeerRoute::Forward { peer_instance_id } =
            self.connection.classify_daemon_route(daemon_instance_id)?
        else {
            return Ok(None);
        };
        log::info!("{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}");
        let slot = self.connection.common_room_slot(rpc_name)?;
        Ok(Some((slot, peer_instance_id)))
    }

    /// The peer's frames for one of the three context reads, or `None` when this daemon serves it.
    ///
    /// [`ConnectionServiceImpl::stream_served_by_peer`] is the whole decision *and* the forward,
    /// addressed at [`SESSION_FILES_SERVICE`] because that is where the peer declares these three —
    /// including its `InvalidArgument` for a daemon id no peer answers to. Routed **before** the
    /// caller is authenticated, as these three were on `connection.ConnectionService`: the caller
    /// is usually a split session's agent host, whose token the codebase host is the one to verify.
    async fn context_served_by_peer<Req, Frame>(
        &self,
        rpc_name: &str,
        request: &Req,
        daemon_instance_id: &str,
    ) -> Result<Option<MpscResultStream<Frame>>, Status>
    where
        Req: prost::Message,
        Frame: prost::Message + Default + Send + 'static,
    {
        Ok(self
            .connection
            .stream_served_by_peer::<Req, Frame>(
                SESSION_FILES_SERVICE,
                rpc_name,
                daemon_instance_id,
                request,
            )
            .await?
            .map(MpscResultStream::from))
    }

    /// The same bump every `connection.ConnectionService` handler makes: in relay mode the idle
    /// monitor shuts the process down, and a client that has moved to this coordinate is still a
    /// client using it.
    fn record_activity(&self) {
        self.connection.record_rpc_activity();
    }
}

#[async_trait]
impl SessionFilesService for PeerRoutedSessionFiles {
    async fn list_session_workflow_files(
        &self,
        request: Request<ListSessionWorkflowFilesRequest>,
    ) -> Result<Response<ListSessionWorkflowFilesResponse>, Status> {
        self.record_activity();
        self.local.list_session_workflow_files(request).await
    }

    async fn read_session_workflow_file(
        &self,
        request: Request<ReadSessionWorkflowFileRequest>,
    ) -> Result<Response<ReadSessionWorkflowFileResponse>, Status> {
        self.record_activity();
        self.local.read_session_workflow_file(request).await
    }

    type StreamContextManifestStream = MpscResultStream<ContextManifestEntry>;

    async fn stream_context_manifest(
        &self,
        request: Request<ContextManifestRequest>,
    ) -> Result<Response<Self::StreamContextManifestStream>, Status> {
        self.record_activity();
        if let Some(frames) = self
            .context_served_by_peer::<_, ContextManifestEntry>(
                "StreamContextManifest",
                request.get_ref(),
                &request.get_ref().daemon_instance_id,
            )
            .await?
        {
            return Ok(Response::new(frames));
        }
        self.local.stream_context_manifest(request).await
    }

    type StreamReadContextFileStream = MpscResultStream<ContextFileChunk>;

    async fn stream_read_context_file(
        &self,
        request: Request<ReadContextFileRequest>,
    ) -> Result<Response<Self::StreamReadContextFileStream>, Status> {
        self.record_activity();
        if let Some(frames) = self
            .context_served_by_peer::<_, ContextFileChunk>(
                "StreamReadContextFile",
                request.get_ref(),
                &request.get_ref().daemon_instance_id,
            )
            .await?
        {
            return Ok(Response::new(frames));
        }
        self.local.stream_read_context_file(request).await
    }

    type StreamReadContextFileBatchStream = MpscResultStream<ContextFileBatchChunk>;

    async fn stream_read_context_file_batch(
        &self,
        request: Request<ReadContextFileBatchRequest>,
    ) -> Result<Response<Self::StreamReadContextFileBatchStream>, Status> {
        self.record_activity();
        if let Some(frames) = self
            .context_served_by_peer::<_, ContextFileBatchChunk>(
                "StreamReadContextFileBatch",
                request.get_ref(),
                &request.get_ref().daemon_instance_id,
            )
            .await?
        {
            return Ok(Response::new(frames));
        }
        self.local.stream_read_context_file_batch(request).await
    }

    async fn upload_session_file_chunk(
        &self,
        request: Request<UploadSessionFileChunkRequest>,
    ) -> Result<Response<UploadSessionFileChunkResponse>, Status> {
        self.record_activity();
        self.local.upload_session_file_chunk(request).await
    }

    async fn list_session_uploads(
        &self,
        request: Request<ListSessionUploadsRequest>,
    ) -> Result<Response<ListSessionUploadsResponse>, Status> {
        self.record_activity();
        self.local.list_session_uploads(request).await
    }

    async fn delete_session_upload(
        &self,
        request: Request<DeleteSessionUploadRequest>,
    ) -> Result<Response<DeleteSessionUploadResponse>, Status> {
        self.record_activity();
        self.local.delete_session_upload(request).await
    }

    async fn upload_staged_attachment_chunk(
        &self,
        request: Request<UploadStagedAttachmentChunkRequest>,
    ) -> Result<Response<UploadStagedAttachmentChunkResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some((slot, peer)) = self.forward_target(
            "UploadStagedAttachmentChunk",
            &req.session_token,
            &req.daemon_instance_id,
        )? {
            let answer =
                crate::livekit_peer_discovery::forward_upload_staged_attachment_chunk_via_livekit(
                    slot, &peer, req,
                )
                .await?;
            return Ok(Response::new(answer));
        }
        self.local.upload_staged_attachment_chunk(request).await
    }

    async fn list_staged_attachments(
        &self,
        request: Request<ListStagedAttachmentsRequest>,
    ) -> Result<Response<ListStagedAttachmentsResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some((slot, peer)) = self.forward_target(
            "ListStagedAttachments",
            &req.session_token,
            &req.daemon_instance_id,
        )? {
            let answer =
                crate::livekit_peer_discovery::forward_list_staged_attachments_via_livekit(
                    slot, &peer, req,
                )
                .await?;
            return Ok(Response::new(answer));
        }
        self.local.list_staged_attachments(request).await
    }

    async fn delete_staged_attachment(
        &self,
        request: Request<DeleteStagedAttachmentRequest>,
    ) -> Result<Response<DeleteStagedAttachmentResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some((slot, peer)) = self.forward_target(
            "DeleteStagedAttachment",
            &req.session_token,
            &req.daemon_instance_id,
        )? {
            let answer =
                crate::livekit_peer_discovery::forward_delete_staged_attachment_via_livekit(
                    slot, &peer, req,
                )
                .await?;
            return Ok(Response::new(answer));
        }
        self.local.delete_staged_attachment(request).await
    }

    async fn read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<ReadHostDocumentResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some((slot, peer)) = self.forward_target(
            "ReadHostDocument",
            &req.session_token,
            &req.daemon_instance_id,
        )? {
            let answer = crate::livekit_peer_discovery::forward_read_host_document_via_livekit(
                slot, &peer, req,
            )
            .await?;
            return Ok(Response::new(answer));
        }
        self.local.read_host_document(request).await
    }

    type StreamReadHostDocumentStream = MpscResultStream<HostDocumentChunk>;

    async fn stream_read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<Self::StreamReadHostDocumentStream>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some((slot, peer)) = self.forward_target(
            "StreamReadHostDocument",
            &req.session_token,
            &req.daemon_instance_id,
        )? {
            // The owning host resolves the document under its own `os_user` mapping and applies its
            // own cap, so nothing is read here. A peer-side failure — or a stream that stops without
            // its terminator — arrives as an error item, terminating this stream.
            let frames =
                crate::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                    slot, &peer, req,
                )
                .await?;
            return Ok(Response::new(MpscResultStream::from(frames)));
        }
        self.local.stream_read_host_document(request).await
    }
}
