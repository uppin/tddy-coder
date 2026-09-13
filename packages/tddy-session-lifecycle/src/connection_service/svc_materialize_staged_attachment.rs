use super::SplitStartFailure;

use crate::{
    connection_service::agent_roster, livekit_peer_discovery::local_instance_id_for_config,
};

use uuid::Uuid;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

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

    /// Start a **split** session: the agent runs here, its worktree lives on `codebase_instance_id`.
    ///
    /// The codebase daemon creates a `workspace` session holding the worktree; this daemon spawns the
    /// agent with no repository on disk and wires it to that worktree through `mcp__tddy-tools__*`
    /// over LiveKit (`docs/ft/daemon/remote-managed-worktree.md`).
    ///
    /// Atomic by construction: everything that can be resolved locally is resolved *before* the peer
    /// is asked to create anything, and any failure after it has done so tears its session back down.
    /// A half-built split session would strand a worktree on a host with no session left to reclaim
    /// it.
    pub(crate) async fn start_split_claude_cli_session(
        &self,
        os_user: &str,
        codebase_instance_id: &str,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // These two ask for work that only exists on the daemon running the *agent*, which a split
        // session has no repository on. A recipe's tooling runs that host's own `transition`, and a
        // sandboxed spawn jails that host's filesystem; neither is a read another host could serve.
        // Refused rather than silently dropped, because a session that came up without its recipe
        // looks exactly like the session that was asked for.
        if !req.recipe.trim().is_empty() {
            return Err(Status::invalid_argument(
                "a workflow recipe needs a repository on the daemon running the agent; it cannot be combined with codebase_daemon_instance_id",
            ));
        }
        // `specialized_agents` and `semantic_index` are *not* refused: neither depends on where the
        // codebase lives. An index indexes a worktree, and the one that counts is the codebase
        // host's, so that host builds it. An agent is placeable on any host — co-located with the
        // authoritative worktree it reads that worktree directly, anywhere else it reads a clone the
        // session worktree sync keeps current — so a split placement only decides which host the
        // roster and the clones end up on, which is the codebase host either way.
        //
        // Resolved here for the references naming *this* daemon, before the peer is asked for
        // anything: those are a request error only this host can see, and refusing one after the
        // codebase host had cut a worktree would mean tearing one down to report a typo. References
        // naming another host are resolved by the daemon that holds the roster, from that host's own
        // view of the common room.
        self.resolve_specialized_agent_defs(&req.specialized_agents)
            .await?;

        let slot = self.common_room_slot("StartSession")?.clone();

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_id = Uuid::now_v7().to_string();

        // The workspace session's id is chosen *here*, before the peer is asked for anything, and
        // travels in the request. Letting the peer name it would make the answer the only way to
        // learn the id — so a forward that errors or times out while the peer goes on building
        // would leave a worktree on that host with nothing pointing at it and no way to name it in
        // a teardown. This is what makes the failure atomic rather than merely usually atomic.
        let codebase_session_id = Uuid::now_v7().to_string();

        // Resolved before the peer is contacted: a room this daemon cannot mint a token for means
        // the agent could never reach its checkout, so nothing should be created for it. The room is
        // *this* session's and is hosted here — this daemon runs the agent, so it is the session's
        // facilitating daemon whether or not the repo turns out to live somewhere else.
        let livekit = crate::split_session::SplitLiveKitRoom::from_config(
            &self.config,
            tddy_daemon_livekit::session_room::session_room_name(&session_id),
        )?;

        let workspace_req = agent_roster::workspace_start_request(
            req,
            &local_instance_id_for_config(&self.config),
            &session_id,
            &codebase_session_id,
        )?;
        let forwarded =
            tddy_daemon_livekit::livekit_peer_discovery::forward_start_session_via_livekit_within(
                &slot,
                codebase_instance_id,
                &workspace_req,
                self.split_forward_deadline(),
            )
            .await;
        let workspace = match forwarded {
            Ok(workspace) => workspace,
            Err(status) => {
                // The peer may have created the session and its worktree, may have failed part-way
                // through, or may never have started — none of which this side can distinguish. The
                // teardown covers all three, because the id was ours to begin with.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::from_forward_error(&status),
                )
                .await;
                return Err(status);
            }
        };
        // A branch another session owns is reported, not created: the peer built nothing, so the
        // conflict travels back to the caller as it would for a co-located start.
        if workspace.branch_conflict.is_some() {
            return Ok(Response::new(workspace));
        }
        let created_session_id = workspace.session_id.trim();
        if created_session_id.is_empty() {
            return Err(Status::internal(format!(
                "daemon {codebase_instance_id} answered StartSession with no session id; the worktree's placement cannot be recorded"
            )));
        }
        if created_session_id != codebase_session_id {
            // A peer that ignored `requested_session_id` cannot give the guarantee above: the next
            // forward it serves slowly would orphan its worktree. Refused rather than accepted with
            // a warning, and the session it did create is torn down under the id it reported.
            self.tear_down_codebase_session(
                &slot,
                codebase_instance_id,
                created_session_id,
                &req.session_token,
                SplitStartFailure::PeerAnswered,
            )
            .await;
            return Err(Status::internal(format!(
                "daemon {codebase_instance_id} created workspace session {created_session_id:?} instead of the requested {codebase_session_id:?}; it does not honour requested_session_id, so a split session's worktree could not be reclaimed after a failed start"
            )));
        }
        // Nothing about the peer's LiveKit fields is checked here any more: a codebase daemon hosts
        // no room. It holds a checkout and answers `GetWorktreeSnapshot` and tool calls about it,
        // both of which this daemon reaches over the peer routing it already uses.
        let started = self
            .spawn_split_agent(
                os_user,
                &session_id,
                &sessions_base,
                codebase_instance_id,
                &codebase_session_id,
                &livekit,
                req,
                progress,
            )
            .await;

        match started {
            Ok(response) => Ok(response),
            Err(status) => {
                // The agent spawn is this daemon's own work: the peer already answered, and
                // whatever it built is there to be reclaimed.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::PeerAnswered,
                )
                .await;
                Err(status)
            }
        }
    }
}
