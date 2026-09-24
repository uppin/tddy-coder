use tddy_service::proto::session::HostDocumentRef;

/// The scope every side of this resolves against — `types.proto`'s, which `connection.proto` and
/// `session_files.proto` both import rather than duplicating, so a `HostDocumentRef` built for a
/// `StartSession` and the `ReadHostDocument` that fetches it name one enum.
use tddy_service::proto::types::HostDocumentScope;

use tddy_service::proto::session_files::ReadHostDocumentRequest;

use crate::session_file_upload::contained_canonical_dir;

use crate::livekit_peer_discovery::PeerRoute;

use crate::session_file_upload::validate_segment;

use tddy_rpc::Status;

use super::AttachmentProgressReporter;

use tddy_service::proto::session::StagedAttachmentRef;

use std::path::Path;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Copies one staged file into the session's attachments.
    ///
    /// The browser stages to whichever daemon it is connected to and may then start the session on
    /// another host, so a ref naming a foreign daemon is fetched from that daemon through the
    /// `STAGED_ATTACHMENT` host-document scope — which applies the containment and
    /// completeness-marker guards on the **owning** side, the only host that can tell a truncated
    /// upload from a whole one. That fetch goes over the *streaming* read
    /// ([`Self::fetch_peer_staged_attachment`]), so crossing hosts does not shrink the size a
    /// session will accept. A local ref is copied straight off disk: there is no reason to
    /// round-trip bytes through RPC on a single host.
    pub(crate) async fn materialize_staged_attachment(
        &self,
        session_token: &str,
        staging_root: &Path,
        session_dir: &Path,
        staged: &StagedAttachmentRef,
        basename: &str,
        progress: &AttachmentProgressReporter<'_>,
    ) -> Result<(), Status> {
        let safe_staging = validate_segment(&staged.staging_id)?;
        let safe_name = validate_segment(&staged.file_name)?;

        match self.classify_daemon_route(&staged.daemon_instance_id)? {
            PeerRoute::Local => Self::copy_local_staged_attachment(
                staging_root,
                session_dir,
                safe_staging,
                safe_name,
                basename,
            ),
            PeerRoute::Forward { peer_instance_id } => {
                self.fetch_peer_staged_attachment(
                    session_token,
                    session_dir,
                    &peer_instance_id,
                    &format!("{safe_staging}/{safe_name}"),
                    basename,
                    progress,
                )
                .await
            }
        }
    }

    /// Copies a staged file that already lives on this host into the session's attachments.
    pub(crate) fn copy_local_staged_attachment(
        staging_root: &Path,
        session_dir: &Path,
        safe_staging: &str,
        safe_name: &str,
        basename: &str,
    ) -> Result<(), Status> {
        let batch_dir = staging_root.join(safe_staging);
        if !batch_dir.exists() {
            return Err(Status::invalid_argument("staged attachment file not found"));
        }
        let canonical_dir = contained_canonical_dir(staging_root, &batch_dir)?;
        let staged_path = canonical_dir.join(safe_name);
        if !staged_path.is_file() {
            return Err(Status::invalid_argument("staged attachment file not found"));
        }
        // The writer only marks a staged file complete on its final chunk; refuse an
        // in-progress or aborted upload so the agent never sees truncated bytes.
        if !crate::session_attachment_staging::staged_complete_marker(&canonical_dir, safe_name)
            .exists()
        {
            return Err(Status::failed_precondition(
                "staged attachment upload is not complete",
            ));
        }

        crate::session_attachments::copy_attachment_into_session(
            session_dir,
            &staged_path,
            basename,
        )?;
        Ok(())
    }

    /// Fetches a staged file from the peer that owns it, over the **streaming** host-document read,
    /// reporting each frame's arrival as progress.
    ///
    /// The unary read carries its own `MAX_HOST_DOCUMENT_BYTES` ceiling — a transport message-size
    /// budget, not a policy. Routing a cross-host staged ref through it would refuse a document
    /// that materializes fine when the session runs on the staging host, so the same attachment
    /// would succeed on one host and fail across two. The stream has no per-message ceiling, which
    /// leaves the host's configured `max_attachment_bytes` as the single limit on both paths.
    ///
    /// This is the slowest thing a start-session request does, and on the feature's primary flow —
    /// bytes staged on the host the browser is connected to, session started on another — it is the
    /// *only* thing between accepting the request and reporting the first byte of work. Reporting
    /// per frame is therefore what keeps a relayed `StreamStartSession` producing inside
    /// [`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`], and what makes the row's
    /// progress bar advance instead of sitting at 0% for the whole transfer.
    pub(crate) async fn fetch_peer_staged_attachment(
        &self,
        session_token: &str,
        session_dir: &Path,
        peer_instance_id: &str,
        staged_relative_path: &str,
        basename: &str,
        progress: &AttachmentProgressReporter<'_>,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("StreamReadHostDocument")?;
        let read_req = ReadHostDocumentRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: peer_instance_id.to_string(),
            scope: HostDocumentScope::StagedAttachment.into(),
            session_id: String::new(),
            project_id: String::new(),
            relative_path: staged_relative_path.to_string(),
        };
        let mut frames =
            tddy_daemon_livekit::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                slot,
                peer_instance_id,
                &read_req,
            )
            .await?;

        // The owning host refuses an over-cap document before its first frame, but a forwarded
        // stream is bytes from a peer — hold the same configured cap here, and stop as soon as it
        // is crossed rather than buffering an unbounded document into memory.
        let max_bytes = self.config.max_attachment_bytes;
        let mut data: Vec<u8> = Vec::new();
        while let Some(frame) = frames.recv().await {
            let frame = frame?;
            data.extend_from_slice(&frame.data);
            if data.len() as u64 > max_bytes {
                return Err(Status::invalid_argument(format!(
                    "staged attachment exceeds this host's maximum attachment size of {max_bytes} bytes"
                )));
            }
            // The peer stamps the whole document's size on every frame, so each one is a complete
            // progress reading with no preamble needed.
            progress.report(data.len() as u64, frame.total_byte_size);
        }

        crate::session_attachments::write_attachment_bytes(session_dir, basename, &data)?;
        Ok(())
    }

    pub(crate) async fn materialize_host_document_attachment(
        &self,
        session_token: &str,
        os_user: &str,
        session_dir: &Path,
        host_doc: &HostDocumentRef,
        basename: &str,
        local_instance_id: &str,
    ) -> Result<(), Status> {
        let scope =
            HostDocumentScope::try_from(host_doc.scope).unwrap_or(HostDocumentScope::Unspecified);
        let ref_daemon = host_doc.daemon_instance_id.trim();

        let bytes = if ref_daemon.is_empty() || ref_daemon == local_instance_id {
            crate::host_documents::read_host_document_bytes(
                os_user,
                &self.tddy_data_dir,
                &self.staging_base_dir,
                scope,
                &host_doc.session_id,
                &host_doc.project_id,
                &host_doc.relative_path,
            )?
        } else {
            let route = self.classify_daemon_route(ref_daemon)?;
            match route {
                PeerRoute::Local => crate::host_documents::read_host_document_bytes(
                    os_user,
                    &self.tddy_data_dir,
                    &self.staging_base_dir,
                    scope,
                    &host_doc.session_id,
                    &host_doc.project_id,
                    &host_doc.relative_path,
                )?,
                PeerRoute::Forward { peer_instance_id } => {
                    let slot = self.common_room_slot("ReadHostDocument")?;
                    let read_req = ReadHostDocumentRequest {
                        session_token: session_token.to_string(),
                        daemon_instance_id: ref_daemon.to_string(),
                        scope: host_doc.scope,
                        session_id: host_doc.session_id.clone(),
                        project_id: host_doc.project_id.clone(),
                        relative_path: host_doc.relative_path.clone(),
                    };
                    let resp =
                        tddy_daemon_livekit::livekit_peer_discovery::forward_read_host_document_via_livekit(
                            slot,
                            &peer_instance_id,
                            &read_req,
                        )
                        .await?;
                    crate::host_documents::HostDocumentBytes {
                        data: resp.data,
                        byte_size: resp.byte_size,
                    }
                }
            }
        };

        // Defense in depth: the owning daemon enforces `MAX_HOST_DOCUMENT_BYTES` on a local
        // read, but a forwarded response is trusted bytes from a peer — re-check the cap on
        // the session host before writing, so a buggy/older peer cannot push an oversized
        // blob into the session's attachments.
        if bytes.data.len() > crate::host_documents::MAX_HOST_DOCUMENT_BYTES {
            return Err(Status::invalid_argument(format!(
                "host document exceeds maximum size of {} bytes",
                crate::host_documents::MAX_HOST_DOCUMENT_BYTES
            )));
        }

        crate::session_attachments::write_attachment_bytes(session_dir, basename, &bytes.data)?;
        Ok(())
    }
}

mod split_claude_cli_start;
