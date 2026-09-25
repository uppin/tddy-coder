use async_trait::async_trait;

use tddy_service::proto::session_files::{
    HostDocumentChunk, ReadHostDocumentResponse, ReadSessionWorkflowFileRequest,
    ReadSessionWorkflowFileResponse, UploadSessionFileChunkRequest, UploadSessionFileChunkResponse,
    UploadStagedAttachmentChunkRequest, UploadStagedAttachmentChunkResponse,
};

use tddy_service::proto::session_files::ReadHostDocumentRequest;

use tddy_service::proto::session_files::DeleteStagedAttachmentResponse;

use tddy_service::proto::session_files::DeleteStagedAttachmentRequest;

use tddy_service::proto::session_files::ListStagedAttachmentsResponse;

use tddy_service::proto::session_files::ListStagedAttachmentsRequest;

use tddy_service::proto::session_files::DeleteSessionUploadResponse;

use tddy_service::proto::session_files::DeleteSessionUploadRequest;

use tddy_service::proto::session_files::ListSessionUploadsResponse;

use tddy_service::proto::session_files::ListSessionUploadsRequest;

use tddy_service::proto::session_files::ReadContextFileBatchRequest;

use tddy_service::proto::session_files::ContextFileBatchChunk;

use tddy_service::proto::session_files::ReadContextFileRequest;

use tddy_service::proto::session_files::ContextFileChunk;

use tddy_service::proto::session_files::ContextManifestRequest;

use tddy_service::proto::session_files::ContextManifestEntry;

use tddy_service::proto::session_files::ListSessionWorkflowFilesResponse;

use tddy_rpc::Response;

use tddy_service::proto::session_files::ListSessionWorkflowFilesRequest;

use tddy_rpc::Request;

use tddy_service::proto::session_files::SessionFilesService;

use super::SESSION_FILES_SERVICE;

use tddy_worktree_service::stream::MpscResultStream;

use crate::livekit_peer_discovery::PeerRoute;

use tddy_rpc::Status;

use super::CommonRoomSlot;

use tddy_session_files::SessionFilesServiceImpl;

use super::super::DaemonSessionHost;

use std::sync::Arc;

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
/// before `#unbundle` node 6 moved these methods off `the pre-unbundle monolithic RPC coordinate`. Routing a
/// step later, at the transport, would leave every in-process caller of the surface serving a
/// request that names another host out of this host's own directories, and would decode each
/// routed request a second time to find the id it routes on.
pub struct PeerRoutedSessionFiles {
    pub(crate) connection: Arc<DaemonSessionHost>,
    /// The `tddy-session-files` implementation, which serves every request this daemon keeps.
    pub(crate) local: SessionFilesServiceImpl,
}

impl PeerRoutedSessionFiles {
    /// The peer one of the five staging and host-document calls is addressed at, with the room to
    /// forward it over — or `None` when the call is this daemon's own to serve.
    ///
    /// The caller is authenticated **first**, which is the order the `the pre-unbundle monolithic RPC coordinate`
    /// handlers these five moved off used: an anonymous request must not be able to drive an
    /// outbound forward and hold a pending-call slot on two hosts for the forward's whole deadline.
    /// The route itself is [`DaemonSessionHost::classify_daemon_route`], refusals and all — the
    /// same decision every other routed RPC on this daemon makes.
    pub(crate) fn forward_target(
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
    /// [`DaemonSessionHost::stream_served_by_peer`] is the whole decision *and* the forward,
    /// addressed at [`SESSION_FILES_SERVICE`] because that is where the peer declares these three —
    /// including its `InvalidArgument` for a daemon id no peer answers to. Routed **before** the
    /// caller is authenticated, as these three were on `the pre-unbundle monolithic RPC coordinate`: the caller
    /// is usually a split session's agent host, whose token the codebase host is the one to verify.
    pub(crate) async fn context_served_by_peer<Req, Frame>(
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

    /// The same bump every `the pre-unbundle monolithic RPC coordinate` handler makes: in relay mode the idle
    /// monitor shuts the process down, and a client that has moved to this coordinate is still a
    /// client using it.
    pub(crate) fn record_activity(&self) {
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
