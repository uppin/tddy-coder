use crate::chunking;
use crate::envelope::decode_request;
use crate::participant::resolve_response_identity;
use crate::participant::LiveKitParticipant;
use crate::participant::RPC_TOPIC;
use crate::proto::RpcResponse;
use crate::rpc_trace;
use livekit::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tddy_changeset::session_participant_metadata::merge_participant_metadata_json;
use tddy_rpc::server_engine::ServerEngine;
use tokio::sync::{mpsc, Mutex};

impl<S: crate::bridge::RpcService> LiveKitParticipant<S> {
    /// Run the participant event loop. Processes DataReceived events for topic "tddy-rpc"
    /// and dispatches to the [`ServerEngine`]. Returns when the room disconnects.
    pub async fn run(mut self) {
        log::info!("[echo_server] LiveKitParticipant event loop started");
        if let Some(ref path) = self.codex_oauth_watch {
            let local = self.room.local_participant().clone();
            let path = path.clone();
            let lock = self.metadata_publish_lock.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                let mut last_sent: Option<String> = None;
                loop {
                    interval.tick().await;
                    Self::try_publish_codex_oauth_metadata(
                        &local,
                        path.as_path(),
                        &mut last_sent,
                        &lock,
                    )
                    .await;
                }
            });
        }
        if let Some(ref projects_dir) = self.projects_registry_dir {
            let local = self.room.local_participant().clone();
            let projects_dir = projects_dir.clone();
            let lock = self.metadata_publish_lock.clone();
            tokio::spawn(async move {
                loop {
                    if let Err(e) = Self::apply_owned_project_count_to_local_metadata(
                        &local,
                        projects_dir.as_path(),
                        &lock,
                    )
                    .await
                    {
                        log::warn!("owned project count metadata: {}", e);
                    }
                    log::debug!(
                        target: "tddy_livekit::metadata",
                        "owned project count: sleeping 30s before next registry poll (bounded refresh)"
                    );
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });
        }
        // Per-sender chunk reassembly. Frames arrive in order per sender on the reliable channel; a
        // raw (non-chunk) payload passes straight through. Keyed by sender identity because message
        // ids are unique only within one sender and several remotes may share this room.
        let mut reassemblers: HashMap<Option<ParticipantIdentity>, chunking::ChunkReassembler> =
            HashMap::new();
        while let Some(event) = self.events.recv().await {
            match event {
                RoomEvent::ConnectionStateChanged(state) => {
                    log::info!("[LiveKit] ConnectionStateChanged {:?}", state);
                }
                RoomEvent::Connected {
                    participants_with_tracks,
                } => {
                    log::info!(
                        "[LiveKit] Connected ({} remote participant(s) with tracks)",
                        participants_with_tracks.len()
                    );
                }
                RoomEvent::Disconnected { reason } => {
                    log::info!("[LiveKit] Disconnected reason={:?}", reason);
                }
                RoomEvent::Reconnecting => {
                    log::info!("[LiveKit] Reconnecting");
                }
                RoomEvent::Reconnected => {
                    log::info!("[LiveKit] Reconnected");
                }
                RoomEvent::ParticipantConnected(remote) => {
                    let identity = remote.identity().clone();
                    log::info!("[LiveKit] ParticipantConnected {:?}", identity);
                    let drained: Vec<_> = {
                        let mut pending = self.pending_data.lock().await;
                        pending.drain(..).collect()
                    };
                    if !drained.is_empty() {
                        log::debug!(
                            "[echo_server] Processing {} buffered payload(s) from {:?}",
                            drained.len(),
                            identity
                        );
                        let remote_identities: Vec<_> =
                            self.room.remote_participants().keys().cloned().collect();
                        for payload in drained {
                            let sender = Some(identity.clone());
                            let Some(full) = Self::reassemble(&mut reassemblers, &sender, payload)
                            else {
                                continue;
                            };
                            if let Err(e) = Self::handle_incoming(
                                &full,
                                sender,
                                &remote_identities,
                                &self.server,
                                &self.outgoing_tx,
                            )
                            .await
                            {
                                log::error!("RPC handle error (buffered): {}", e);
                            }
                        }
                    }
                }
                RoomEvent::DataReceived {
                    payload,
                    topic,
                    kind: _,
                    participant,
                } => {
                    if topic.as_deref() != Some(RPC_TOPIC) {
                        continue;
                    }
                    let (event_participant, remote_identities): (
                        Option<ParticipantIdentity>,
                        Vec<ParticipantIdentity>,
                    ) = match &participant {
                        Some(remote) => {
                            let identity = remote.identity().clone();
                            let remotes: Vec<_> =
                                self.room.remote_participants().keys().cloned().collect();
                            (Some(identity), remotes)
                        }
                        None => {
                            let remotes: Vec<_> =
                                self.room.remote_participants().keys().cloned().collect();
                            if remotes.len() == 1 {
                                log::debug!(
                                    "[echo_server] DataReceived without participant, using sole remote {:?}",
                                    remotes[0]
                                );
                                (Some(remotes[0].clone()), remotes)
                            } else if remotes.is_empty() {
                                log::debug!(
                                    "[echo_server] DataReceived without participant (remotes=0), buffering"
                                );
                                let bytes =
                                    Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                                self.pending_data.lock().await.push_back(bytes);
                                continue;
                            } else {
                                log::debug!(
                                    "[echo_server] DataReceived without participant (remotes={}), proceeding with sender_identity from request",
                                    remotes.len()
                                );
                                (None, remotes)
                            }
                        }
                    };
                    log::info!(
                        "[echo_server] RPC received from {:?} ({} bytes)",
                        event_participant,
                        payload.len()
                    );
                    rpc_trace!(
                        "LiveKitParticipant: incoming RPC payload from {:?} ({} bytes)",
                        event_participant,
                        payload.len()
                    );
                    let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                    let Some(full) =
                        Self::reassemble(&mut reassemblers, &event_participant, payload)
                    else {
                        continue;
                    };

                    // Must not `spawn` per packet: `ServerEngine::on_request` may itself spawn
                    // the actual dispatch, but decoding + routing to an existing bidi/multi-message
                    // session must stay in arrival order (concurrent tasks could complete out of
                    // order, reordering keystrokes on `StreamTerminalIO` and any other bidi stream).
                    if let Err(e) = Self::handle_incoming(
                        &full,
                        event_participant,
                        &remote_identities,
                        &self.server,
                        &self.outgoing_tx,
                    )
                    .await
                    {
                        log::error!("RPC handle error: {}", e);
                    }
                }
                RoomEvent::ParticipantDisconnected(remote) => {
                    let identity = remote.identity();
                    log::info!("[LiveKit] ParticipantDisconnected {:?}", identity);
                    // Responses are addressed by participant identity (see `handle_incoming`), so
                    // that identity is also the key under which the engine holds this peer's live
                    // bidi sessions, half-sent calls and in-flight streaming forwards. Without this
                    // a departed peer's forwards keep pumping frames nobody will ever read.
                    self.server.on_peer_disconnected(identity.as_str()).await;
                }
                _ => {}
            }
        }
        log::debug!("LiveKitParticipant::run event loop ended");
    }

    async fn try_publish_codex_oauth_metadata(
        local: &LocalParticipant,
        path: &Path,
        last_sent: &mut Option<String>,
        metadata_publish_lock: &Arc<Mutex<()>>,
    ) {
        let Ok(raw) = tokio::fs::read_to_string(path).await else {
            return;
        };
        let url = raw.trim().to_string();
        if !url.starts_with("https://") {
            return;
        }
        if last_sent.as_ref() == Some(&url) {
            return;
        }
        let (callback_port, state) =
            tddy_service::codex_oauth_scan::codex_oauth_from_authorize_url_only(&url)
                .map(|d| (d.callback_port, d.state))
                .unwrap_or((1455, String::new()));
        let update = serde_json::json!({
            "codex_oauth": {
                "pending": true,
                "authorize_url": url,
                "callback_port": callback_port,
                "state": state,
            }
        })
        .to_string();

        let _guard = metadata_publish_lock.lock().await;
        let baseline = local.metadata();
        let merged = match merge_participant_metadata_json(&baseline, &update) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    target: "tddy_livekit::codex_oauth",
                    "merge before set_metadata failed: {}",
                    e
                );
                return;
            }
        };
        match local.set_metadata(merged).await {
            Ok(()) => {
                *last_sent = Some(url);
                log::info!(
                    target: "tddy_livekit::codex_oauth",
                    "published Codex OAuth pending merged into participant metadata (URL omitted from logs)"
                );
            }
            Err(e) => {
                log::warn!(
                    target: "tddy_livekit::codex_oauth",
                    "set_metadata failed: {}",
                    e
                );
            }
        }
    }

    /// Reassemble one received frame from `sender` into a whole RPC payload. A raw (non-chunk)
    /// payload is returned as-is; a chunk frame is buffered in `sender`'s reassembler until its
    /// message completes. Returns `None` while chunks are still outstanding, or when a frame is
    /// malformed (logged and dropped rather than crashing the loop).
    fn reassemble(
        reassemblers: &mut HashMap<Option<ParticipantIdentity>, chunking::ChunkReassembler>,
        sender: &Option<ParticipantIdentity>,
        payload: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if !chunking::is_chunk_frame(&payload) {
            return Some(payload);
        }
        match reassemblers
            .entry(sender.clone())
            .or_default()
            .accept(&payload)
        {
            Ok(result) => result,
            Err(e) => {
                log::warn!(
                    "[echo_server] malformed chunk frame from {:?}: {}",
                    sender,
                    e
                );
                None
            }
        }
    }

    /// Decode one incoming RPC frame and route it to [`ServerEngine::on_request`], addressed to
    /// the resolved response identity. Multiplexing (bidi sessions, client-streaming message
    /// accumulation, dispatch) all happen inside `ServerEngine`; this function's only job is
    /// LiveKit-specific: figuring out *who* to reply to.
    async fn handle_incoming(
        payload: &[u8],
        event_participant: Option<ParticipantIdentity>,
        remote_identities: &[ParticipantIdentity],
        server: &Arc<ServerEngine<S>>,
        outgoing: &mpsc::Sender<(String, RpcResponse)>,
    ) -> Result<(), String> {
        let request = decode_request(payload)?;
        if let Some(cm) = request.call_metadata.as_ref() {
            log::info!(
                "[rpc] incoming call {}/{} request_id={} ({} bytes) from={:?}",
                cm.service,
                cm.method,
                request.request_id,
                payload.len(),
                event_participant,
            );
        }
        let response_identity =
            resolve_response_identity(&request, event_participant.clone(), remote_identities)
                .ok_or_else(|| {
                    "no response destination (sender_identity absent, multiple remotes)".to_string()
                })?;
        rpc_trace!(
            "LiveKitParticipant::handle_incoming request_id={} end_of_stream={} event_participant={:?} response_identity={:?}",
            request.request_id,
            request.end_of_stream,
            event_participant,
            response_identity
        );
        server
            .on_request(response_identity.as_str(), request, outgoing.clone())
            .await;
        Ok(())
    }
}
