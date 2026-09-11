//! What this daemon hands `tddy-session-files` so that crate can serve
//! `session_files.SessionFilesService`, and the routing it keeps for itself.
//!
//! The thirteen session-file methods are `tddy-session-files`'; what stays here is the six answers
//! only a daemon has — which OS user a session token belongs to, where this host keeps its data and
//! its staging area, what it caps an attachment at, which instance id it stamps on a staged entry,
//! and which checkout a session's agent guidance is read from.
//!
//! Eight of the thirteen also **route**: a request naming another daemon is served by that daemon,
//! not here. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-files` may reach for — its module header says so
//! — which is why [`PeerRoutedSessionFiles`] wraps the crate's entry rather than the crate growing a
//! transport. Every decision below is made by the *same* [`ConnectionServiceImpl`] method the
//! matching `connection.ConnectionService` handler calls, so the two coordinates cannot disagree
//! about which host holds a file.

use std::sync::Arc;

use async_trait::async_trait;
use livekit::prelude::Room;
use prost::Message as _;
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::connection::{
    ContextFileBatchChunk, ContextFileChunk, ContextManifestEntry, ContextManifestRequest,
    DeleteStagedAttachmentRequest, ExecuteToolRequest, HostDocumentChunk,
    ListStagedAttachmentsRequest, ReadContextFileBatchRequest, ReadContextFileRequest,
    ReadHostDocumentRequest, UploadStagedAttachmentChunkRequest,
};
use tddy_session_files::service::{SessionContextScope, SessionContextScopes};
use tddy_session_files::SessionFilesPorts;
use tokio::sync::mpsc;

use super::ConnectionServiceImpl;
use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};

/// The coordinate this module mounts. Compared against the dispatched service name so a call that
/// somehow arrived for another service is refused by the crate's own server rather than routed.
const SESSION_FILES_SERVICE: &str = "session_files.SessionFilesService";

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
    /// Built from the *same* config, resolvers and staging base `connection.ConnectionService`
    /// serves its session-file RPCs from, so both coordinates address one staging directory, one
    /// OS-user mapping and one set of sessions while both are mounted — a second staging base here
    /// would mean a batch uploaded on one coordinate was invisible to the start addressed through
    /// the other.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn session_files_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        let entry = tddy_session_files::build_session_files_entry(self.session_files_ports());
        tddy_rpc::ServiceEntry {
            name: entry.name,
            service: Arc::new(PeerRoutedSessionFiles {
                connection: Arc::clone(self),
                local: entry.service,
            }) as Arc<dyn RpcService>,
        }
    }

    /// The six host answers the thirteen handlers need, each read off this daemon.
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
        let (sessions_base, worktree_root) =
            self.connection
                .resolve_exec_tool_worktree(&ExecuteToolRequest {
                    session_token: session_token.to_string(),
                    session_id: session_id.to_string(),
                    tool_name: CONTEXT_SCOPE_CALLER.to_string(),
                    args_json: String::new(),
                    daemon_instance_id: String::new(),
                })?;
        let globs = self.connection.context_globs_for_session(
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

/// The crate's entry, with the eight routed methods answered by the daemon that holds the bytes.
///
/// The wrapper is *this* side of the boundary on purpose: `HostDocumentPicker` browses another
/// host by `browsedDaemonInstanceId`, and a coordinate that served every request locally would
/// answer such a browse with this host's files — an empty or refused list that a client cannot
/// tell from a genuinely empty directory.
struct PeerRoutedSessionFiles {
    connection: Arc<ConnectionServiceImpl>,
    /// The `tddy-session-files` entry, which serves every request this daemon keeps.
    local: Arc<dyn RpcService>,
}

impl PeerRoutedSessionFiles {
    /// The peer a request names, with the request decoded and the room to forward it over.
    ///
    /// `Ok(None)` means the call is this daemon's own to serve. The decision is
    /// [`ConnectionServiceImpl::classify_daemon_route`] — the one the five staging and
    /// host-document handlers on `connection.ConnectionService` make, refusals and all.
    ///
    /// `Req` is the `connection` proto twin of the `session_files` message the caller sent. The two
    /// are the same wire shape (node 6 copied them field for field), the forward is addressed to
    /// the peer's `connection.ConnectionService`, and so the bytes are decoded once as the type
    /// that coordinate will decode them as — nothing is converted between two spellings of one
    /// message.
    fn peer_forward<Req>(
        &self,
        rpc_name: &str,
        payload: &[u8],
        daemon_instance_id: fn(&Req) -> &str,
    ) -> Result<Option<(&CommonRoomSlot, String, Req)>, Status>
    where
        Req: prost::Message + Default,
    {
        let request = Req::decode(payload).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let PeerRoute::Forward { peer_instance_id } = self
            .connection
            .classify_daemon_route(daemon_instance_id(&request))?
        else {
            return Ok(None);
        };
        log::info!("{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}");
        let slot = self.connection.common_room_slot(rpc_name)?;
        Ok(Some((slot, peer_instance_id, request)))
    }

    /// A context RPC the daemon addressed by the request should answer.
    ///
    /// `None` means this daemon serves it. [`ConnectionServiceImpl::stream_served_by_peer`] is the
    /// whole decision *and* the forward — the same call the three context handlers on
    /// `connection.ConnectionService` make, including its `InvalidArgument` for a daemon id no peer
    /// answers to.
    async fn context_served_by_peer<Req, Frame>(
        &self,
        rpc_name: &str,
        payload: &[u8],
        daemon_instance_id: fn(&Req) -> &str,
    ) -> Option<RpcResult>
    where
        Req: prost::Message + Default,
        Frame: prost::Message + Default + Send + 'static,
    {
        let request = match Req::decode(payload) {
            Ok(request) => request,
            Err(e) => {
                return Some(RpcResult::ServerStream(Err(Status::invalid_argument(
                    e.to_string(),
                ))))
            }
        };
        match self
            .connection
            .stream_served_by_peer::<Req, Frame>(rpc_name, daemon_instance_id(&request), &request)
            .await
        {
            Ok(None) => None,
            Ok(Some(frames)) => Some(relayed(frames)),
            Err(status) => Some(RpcResult::ServerStream(Err(status))),
        }
    }

    /// The peer's answer to one of the eight routed methods, or `None` when this daemon serves it.
    async fn served_by_peer(&self, method: &str, payload: &[u8]) -> Option<RpcResult> {
        match method {
            "StreamContextManifest" => {
                self.context_served_by_peer::<ContextManifestRequest, ContextManifestEntry>(
                    method,
                    payload,
                    |req| &req.daemon_instance_id,
                )
                .await
            }
            "StreamReadContextFile" => {
                self.context_served_by_peer::<ReadContextFileRequest, ContextFileChunk>(
                    method,
                    payload,
                    |req| &req.daemon_instance_id,
                )
                .await
            }
            "StreamReadContextFileBatch" => {
                self.context_served_by_peer::<ReadContextFileBatchRequest, ContextFileBatchChunk>(
                    method,
                    payload,
                    |req| &req.daemon_instance_id,
                )
                .await
            }
            "UploadStagedAttachmentChunk" => {
                match self.peer_forward::<UploadStagedAttachmentChunkRequest>(
                    method,
                    payload,
                    |req| &req.daemon_instance_id,
                ) {
                    Ok(None) => None,
                    Ok(Some((slot, peer, request))) => Some(RpcResult::Unary(
                        crate::livekit_peer_discovery::forward_upload_staged_attachment_chunk_via_livekit(
                            slot, &peer, &request,
                        )
                        .await
                        .map(|answer| answer.encode_to_vec()),
                    )),
                    Err(status) => Some(RpcResult::Unary(Err(status))),
                }
            }
            "ListStagedAttachments" => {
                match self.peer_forward::<ListStagedAttachmentsRequest>(method, payload, |req| {
                    &req.daemon_instance_id
                }) {
                    Ok(None) => None,
                    Ok(Some((slot, peer, request))) => Some(RpcResult::Unary(
                        crate::livekit_peer_discovery::forward_list_staged_attachments_via_livekit(
                            slot, &peer, &request,
                        )
                        .await
                        .map(|answer| answer.encode_to_vec()),
                    )),
                    Err(status) => Some(RpcResult::Unary(Err(status))),
                }
            }
            "DeleteStagedAttachment" => {
                match self.peer_forward::<DeleteStagedAttachmentRequest>(method, payload, |req| {
                    &req.daemon_instance_id
                }) {
                    Ok(None) => None,
                    Ok(Some((slot, peer, request))) => Some(RpcResult::Unary(
                        crate::livekit_peer_discovery::forward_delete_staged_attachment_via_livekit(
                            slot, &peer, &request,
                        )
                        .await
                        .map(|answer| answer.encode_to_vec()),
                    )),
                    Err(status) => Some(RpcResult::Unary(Err(status))),
                }
            }
            "ReadHostDocument" => {
                match self.peer_forward::<ReadHostDocumentRequest>(method, payload, |req| {
                    &req.daemon_instance_id
                }) {
                    Ok(None) => None,
                    Ok(Some((slot, peer, request))) => Some(RpcResult::Unary(
                        crate::livekit_peer_discovery::forward_read_host_document_via_livekit(
                            slot, &peer, &request,
                        )
                        .await
                        .map(|answer| answer.encode_to_vec()),
                    )),
                    Err(status) => Some(RpcResult::Unary(Err(status))),
                }
            }
            "StreamReadHostDocument" => {
                match self.peer_forward::<ReadHostDocumentRequest>(method, payload, |req| {
                    &req.daemon_instance_id
                }) {
                    Ok(None) => None,
                    // The owning host resolves the document under its own `os_user` mapping and
                    // applies its own cap, so nothing is read here.
                    Ok(Some((slot, peer, request))) => Some(
                        match crate::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                            slot, &peer, &request,
                        )
                        .await
                        {
                            Ok(frames) => relayed::<HostDocumentChunk>(frames),
                            Err(status) => RpcResult::ServerStream(Err(status)),
                        },
                    ),
                    Err(status) => Some(RpcResult::ServerStream(Err(status))),
                }
            }
            _ => None,
        }
    }
}

#[async_trait]
impl RpcService for PeerRoutedSessionFiles {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        self.local.is_bidi_stream(service, method)
    }

    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        // The same bump every `connection.ConnectionService` handler makes: in relay mode the idle
        // monitor shuts the process down, and a client that has moved to this coordinate is still
        // a client using it.
        self.connection.record_rpc_activity();
        if service == SESSION_FILES_SERVICE {
            if let Some(answered_by_peer) = self.served_by_peer(method, &message.payload).await {
                return answered_by_peer;
            }
        }
        self.local.handle_rpc(service, method, message).await
    }

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        // A single-message stream is the server-streaming case, and it routes like any other call;
        // anything else is the crate's to refuse, exactly as it does on the unwrapped entry.
        if messages.len() == 1 {
            return self.handle_rpc(service, method, &messages[0]).await;
        }
        self.local
            .handle_rpc_stream(service, method, messages)
            .await
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<tddy_rpc::BidiStreamOutput, Status> {
        self.local
            .start_bidi_stream(service, method, input_rx)
            .await
    }
}

/// A peer's frames, encoded onto the bounded channel the transport drains.
///
/// The relay ends on a closed receiver rather than filling a channel nobody reads, and an error
/// item from the peer is carried through as one — a stream that stopped short must not read as a
/// short answer.
fn relayed<Frame>(
    mut frames: tokio::sync::mpsc::UnboundedReceiver<Result<Frame, Status>>,
) -> RpcResult
where
    Frame: prost::Message + Send + 'static,
{
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            if tx.send(frame.map(|f| f.encode_to_vec())).await.is_err() {
                break;
            }
        }
    });
    RpcResult::ServerStream(Ok(rx))
}
