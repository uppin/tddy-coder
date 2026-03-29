//! LiveKit room participant that serves RPC over the data channel.

use livekit::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

use tddy_rpc::{RequestMetadata, RpcMessage};

use crate::bridge::{ResponseBody, RpcBridge};
use crate::envelope::{decode_request, encode_response, response_from_result};
use crate::proto::{CallMetadata, RpcRequest};
use crate::rpc_trace;
use crate::token::TokenGenerator;

const RPC_TOPIC: &str = "tddy-rpc";

/// Composite key for multiplexing RPC streams per remote client (request_id alone is not unique across tabs).
type SessionKey = (String, i32);

/// Accumulated stream state: sender identity and messages.
struct ActiveStream {
    sender_identity: ParticipantIdentity,
    messages: Vec<RpcRequest>,
}

/// Bidi stream metadata: service and method for continuation messages that omit call_metadata.
struct BidiStreamMeta {
    service: String,
    method: String,
}

/// Live bidi session: the input channel to an already-running handler (from `start_bidi_stream`).
struct BidiSession {
    input_tx: mpsc::Sender<RpcMessage>,
}

/// Shared publisher that survives room reconnection cycles.
/// Output tasks hold a clone and retry publishing through the latest `LocalParticipant`
/// when the room reconnects during token refresh.
#[derive(Clone)]
pub(crate) struct SharedPublisher {
    local: Arc<Mutex<Option<LocalParticipant>>>,
    notify: Arc<tokio::sync::Notify>,
}

impl SharedPublisher {
    fn new() -> Self {
        Self {
            local: Arc::new(Mutex::new(None)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    async fn update(&self, lp: LocalParticipant) {
        *self.local.lock().await = Some(lp);
        self.notify.notify_waiters();
    }

    /// Publish data, retrying with the latest LocalParticipant during reconnection gaps.
    async fn publish_data(
        &self,
        payload: Vec<u8>,
        destination_identities: &[ParticipantIdentity],
    ) -> Result<(), String> {
        for attempt in 0..30 {
            let local = { self.local.lock().await.clone() };
            if let Some(lp) = local {
                let packet = DataPacket {
                    payload: payload.clone(),
                    topic: Some(RPC_TOPIC.to_string()),
                    reliable: true,
                    destination_identities: destination_identities.to_vec(),
                };
                if lp.publish_data(packet).await.is_ok() {
                    return Ok(());
                }
                if attempt == 0 {
                    log::debug!("[reconnect] publish_data failed, waiting for new participant");
                }
            }
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
        Err("publish_data failed after 30 retries during reconnection".to_string())
    }
}

/// A LiveKit room participant that routes RPC traffic to an RpcBridge.
pub struct LiveKitParticipant<S: crate::bridge::RpcService> {
    room: Room,
    bridge: Arc<RpcBridge<S>>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    active_streams: Arc<Mutex<HashMap<SessionKey, ActiveStream>>>,
    /// Bidi stream request_ids and their service/method (continuation messages omit call_metadata).
    active_bidi: Arc<Mutex<HashMap<SessionKey, BidiStreamMeta>>>,
    /// Live bidi sessions: (sender_identity, request_id) → input channel for an already-started handler.
    active_bidi_sessions: Arc<Mutex<HashMap<SessionKey, BidiSession>>>,
    /// Payloads received with participant=None before any remote joined (race with ParticipantConnected).
    pending_data: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// When set, bidi output tasks publish through this instead of a direct LocalParticipant,
    /// allowing them to survive room reconnection during token refresh.
    shared_publisher: Option<SharedPublisher>,
}

impl<S: crate::bridge::RpcService> LiveKitParticipant<S> {
    /// Connect to a LiveKit room and create a participant that will serve RPC.
    pub async fn connect(
        url: &str,
        token: &str,
        service: S,
        room_options: RoomOptions,
    ) -> Result<Self, livekit::RoomError> {
        log::debug!("LiveKitParticipant::connect url={}", url);
        let bridge = RpcBridge::new(service);
        let (room, events) = Room::connect(url, token, room_options).await?;
        log::info!(
            "[echo_server] LiveKitParticipant connected, identity={:?}",
            room.local_participant().identity()
        );
        Ok(Self {
            room,
            bridge: Arc::new(bridge),
            events,
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            active_bidi: Arc::new(Mutex::new(HashMap::new())),
            active_bidi_sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_data: Arc::new(Mutex::new(VecDeque::new())),
            shared_publisher: None,
        })
    }

    /// Connect to a LiveKit room, sharing bidi session state and publisher across
    /// reconnection cycles. The SharedPublisher is updated with the new LocalParticipant
    /// so that output tasks from previous cycles can publish through the new room.
    async fn connect_for_reconnect(
        url: &str,
        token: &str,
        bridge: Arc<RpcBridge<S>>,
        room_options: RoomOptions,
        active_bidi: Arc<Mutex<HashMap<SessionKey, BidiStreamMeta>>>,
        active_bidi_sessions: Arc<Mutex<HashMap<SessionKey, BidiSession>>>,
        shared_publisher: SharedPublisher,
    ) -> Result<Self, livekit::RoomError> {
        log::debug!("LiveKitParticipant::connect_for_reconnect url={}", url);
        let (room, events) = Room::connect(url, token, room_options).await?;
        log::info!(
            "[echo_server] LiveKitParticipant connected (reconnect), identity={:?}",
            room.local_participant().identity()
        );
        shared_publisher
            .update(room.local_participant().clone())
            .await;
        Ok(Self {
            room,
            bridge,
            events,
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            active_bidi,
            active_bidi_sessions,
            pending_data: Arc::new(Mutex::new(VecDeque::new())),
            shared_publisher: Some(shared_publisher),
        })
    }

    /// Run the participant with automatic token refresh. Generates a token, connects,
    /// runs the event loop until TTL-60s elapses, then reconnects with a fresh token.
    /// Exits when the room disconnects or when `shutdown` becomes true.
    pub async fn run_with_reconnect(
        url: &str,
        token_generator: &TokenGenerator,
        service: S,
        room_options: RoomOptions,
        shutdown: Arc<AtomicBool>,
    ) {
        let bridge = Arc::new(RpcBridge::new(service));
        let shared_publisher = SharedPublisher::new();
        let active_bidi: Arc<Mutex<HashMap<SessionKey, BidiStreamMeta>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let active_bidi_sessions: Arc<Mutex<HashMap<SessionKey, BidiSession>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut cycle: u64 = 0;
        loop {
            cycle += 1;
            let token = match token_generator.generate() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Token generation failed: {}", e);
                    return;
                }
            };
            let participant = match Self::connect_for_reconnect(
                url,
                &token,
                bridge.clone(),
                room_options.clone(),
                active_bidi.clone(),
                active_bidi_sessions.clone(),
                shared_publisher.clone(),
            )
            .await
            {
                Ok(p) => {
                    log::info!("READY");
                    p
                }
                Err(e) => {
                    log::error!("LiveKit connect failed: {}", e);
                    return;
                }
            };
            let refresh_delay = token_generator.time_until_refresh();
            log::info!(
                "[reconnect] cycle={} refresh_delay={:?} ttl={:?}",
                cycle,
                refresh_delay,
                token_generator.ttl()
            );
            let bidi_sessions_ref = participant.active_bidi_sessions.clone();
            let active_streams_ref = participant.active_streams.clone();
            let shutdown_clone = shutdown.clone();
            tokio::select! {
                _ = participant.run() => {
                    log::info!("[reconnect] cycle={} participant.run() returned (disconnected)", cycle);
                    break;
                }
                _ = tokio::time::sleep(refresh_delay) => {
                    let bidi_count = bidi_sessions_ref.lock().await.len();
                    let stream_count = active_streams_ref.lock().await.len();
                    log::warn!(
                        "[reconnect] cycle={} token expiring — reconnecting with {} active bidi session(s), {} active stream(s)",
                        cycle,
                        bidi_count,
                        stream_count,
                    );
                    continue;
                }
                _ = async {
                    while !shutdown_clone.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                } => {
                    log::info!("[reconnect] cycle={} shutdown requested", cycle);
                    break;
                }
            }
        }
    }

    /// Run the participant event loop. Processes DataReceived events for topic "tddy-rpc"
    /// and dispatches to the RpcBridge. Returns when the room disconnects.
    pub async fn run(mut self) {
        log::info!("[echo_server] LiveKitParticipant event loop started");
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
                            if let Err(e) = Self::handle_incoming(
                                &payload,
                                Some(identity.clone()),
                                &remote_identities,
                                &self.bridge,
                                &self.room.local_participant(),
                                &self.active_streams,
                                &self.active_bidi,
                                &self.active_bidi_sessions,
                                &self.shared_publisher,
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
                    let bridge = self.bridge.clone();
                    let local = self.room.local_participant();
                    let active_streams = self.active_streams.clone();
                    let active_bidi = self.active_bidi.clone();
                    let active_bidi_sessions = self.active_bidi_sessions.clone();
                    let shared_publisher = self.shared_publisher.clone();
                    let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_incoming(
                            &payload,
                            event_participant,
                            &remote_identities,
                            &bridge,
                            &local,
                            &active_streams,
                            &active_bidi,
                            &active_bidi_sessions,
                            &shared_publisher,
                        )
                        .await
                        {
                            log::error!("RPC handle error: {}", e);
                        }
                    });
                }
                RoomEvent::ParticipantDisconnected(remote) => {
                    log::info!("[LiveKit] ParticipantDisconnected {:?}", remote.identity());
                }
                _ => {}
            }
        }
        let bidi_count = self.active_bidi_sessions.lock().await.len();
        let stream_count = self.active_streams.lock().await.len();
        log::debug!(
            "LiveKitParticipant::run event loop ended (active_bidi_sessions={}, active_streams={})",
            bidi_count,
            stream_count,
        );
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_incoming(
        payload: &[u8],
        event_participant: Option<ParticipantIdentity>,
        remote_identities: &[ParticipantIdentity],
        bridge: &Arc<RpcBridge<S>>,
        local: &LocalParticipant,
        active_streams: &Mutex<HashMap<SessionKey, ActiveStream>>,
        active_bidi: &Mutex<HashMap<SessionKey, BidiStreamMeta>>,
        active_bidi_sessions: &Mutex<HashMap<SessionKey, BidiSession>>,
        shared_publisher: &Option<SharedPublisher>,
    ) -> Result<(), String> {
        let request = decode_request(payload)?;
        let request_id = request.request_id;
        let sender_id = request.sender_identity.clone().unwrap_or_default();
        let session_key = (sender_id, request_id);
        let meta = request.call_metadata.as_ref();
        let end_of_stream = request.end_of_stream;
        rpc_trace!(
            "LiveKitParticipant::handle_incoming request_id={} {}/{} end_of_stream={} event_participant={:?}",
            request_id,
            meta.map(|m| m.service.as_str()).unwrap_or("?"),
            meta.map(|m| m.method.as_str()).unwrap_or("?"),
            end_of_stream,
            event_participant
        );

        // Check if this message belongs to an existing bidi session.
        {
            let mut sessions = active_bidi_sessions.lock().await;
            if let Some(session) = sessions.get(&session_key) {
                let payload_len = request.request_message.len();
                let rpc_msg = RpcMessage {
                    payload: request.request_message.clone(),
                    metadata: RequestMetadata {
                        sender_identity: request.sender_identity.clone(),
                    },
                };
                log::trace!(
                    "[BIDI_TRACE] participant: routing request_id={} to existing bidi session (payload_len={}, end_of_stream={})",
                    request_id, payload_len, end_of_stream
                );
                match session.input_tx.send(rpc_msg).await {
                    Ok(_) => log::trace!(
                        "[BIDI_TRACE] participant: input_tx.send OK request_id={}",
                        request_id
                    ),
                    Err(e) => log::error!(
                        "[BIDI_TRACE] participant: input_tx.send FAILED request_id={}: {}",
                        request_id,
                        e
                    ),
                }
                if end_of_stream {
                    log::trace!(
                        "[BIDI_TRACE] participant: removing bidi session request_id={} (end_of_stream)", request_id
                    );
                    sessions.remove(&session_key);
                    let mut bidi = active_bidi.lock().await;
                    bidi.remove(&session_key);
                }
                return Ok(());
            }
        }

        let to_process: Option<(Vec<RpcRequest>, ParticipantIdentity)> = {
            let mut streams = active_streams.lock().await;
            let mut bidi = active_bidi.lock().await;
            if end_of_stream {
                if let Some(mut stream) = streams.remove(&session_key) {
                    stream.messages.push(request);
                    Some((stream.messages, stream.sender_identity))
                } else {
                    let response_identity = resolve_response_identity(
                        &request,
                        event_participant.clone(),
                        remote_identities,
                    )
                    .ok_or_else(|| {
                        "no response destination (sender_identity absent, multiple remotes)"
                            .to_string()
                    })?;
                    let request_for_bridge = if request.call_metadata.is_none() {
                        if let Some(meta) = bidi.remove(&session_key) {
                            let mut req = request.clone();
                            req.call_metadata = Some(CallMetadata {
                                service: meta.service,
                                method: meta.method,
                            });
                            req
                        } else {
                            request
                        }
                    } else {
                        request
                    };
                    Some((vec![request_for_bridge], response_identity))
                }
            } else {
                let service = request
                    .call_metadata
                    .as_ref()
                    .map(|m| m.service.as_str())
                    .unwrap_or("");
                let method = request
                    .call_metadata
                    .as_ref()
                    .map(|m| m.method.as_str())
                    .unwrap_or("");
                let is_bidi = if request.call_metadata.is_some() {
                    bridge.is_bidi_stream(service, method)
                } else {
                    bidi.contains_key(&session_key)
                };
                if is_bidi {
                    let response_identity = resolve_response_identity(
                        &request,
                        event_participant.clone(),
                        remote_identities,
                    )
                    .ok_or_else(|| {
                        "no response destination for stream (sender_identity absent, multiple remotes)"
                            .to_string()
                    })?;
                    if let Some(meta) = request.call_metadata.as_ref() {
                        bidi.insert(
                            session_key.clone(),
                            BidiStreamMeta {
                                service: meta.service.clone(),
                                method: meta.method.clone(),
                            },
                        );
                    }
                    let service_name = bidi
                        .get(&session_key)
                        .map(|m| m.service.clone())
                        .unwrap_or_default();
                    let method_name = bidi
                        .get(&session_key)
                        .map(|m| m.method.clone())
                        .unwrap_or_default();

                    let (input_tx, input_rx) = mpsc::channel::<RpcMessage>(64);

                    let first_msg = RpcMessage {
                        payload: request.request_message.clone(),
                        metadata: RequestMetadata {
                            sender_identity: request.sender_identity.clone(),
                        },
                    };
                    log::trace!(
                        "[BIDI_TRACE] participant: creating NEW bidi session request_id={} service={} method={} first_payload_len={}",
                        request_id, service_name, method_name, first_msg.payload.len()
                    );
                    let _ = input_tx.send(first_msg).await;

                    {
                        let mut sessions = active_bidi_sessions.lock().await;
                        sessions.insert(session_key.clone(), BidiSession { input_tx });
                    }

                    let bridge = bridge.clone();
                    let local = local.clone();
                    let shared_publisher = shared_publisher.clone();
                    tokio::spawn(async move {
                        log::trace!(
                            "[BIDI_TRACE] participant: calling bridge.start_bidi_stream request_id={}", request_id
                        );
                        match bridge
                            .start_bidi_stream(&service_name, &method_name, input_rx)
                            .await
                        {
                            Ok(handle) => {
                                log::trace!(
                                    "[BIDI_TRACE] participant: bridge.start_bidi_stream OK request_id={}, spawning streaming response", request_id
                                );
                                Self::spawn_streaming_response(
                                    request_id,
                                    handle.output,
                                    local,
                                    response_identity,
                                    true,
                                    shared_publisher,
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "[BIDI_TRACE] participant: start_bidi_stream FAILED request_id={}: {}",
                                    request_id,
                                    e
                                );
                            }
                        }
                    });
                    return Ok(());
                } else if request.call_metadata.is_some() {
                    let sender_identity = resolve_response_identity(
                        &request,
                        event_participant.clone(),
                        remote_identities,
                    )
                    .ok_or_else(|| {
                        "no response destination for stream (sender_identity absent, multiple remotes)"
                            .to_string()
                    })?;
                    streams.insert(
                        session_key.clone(),
                        ActiveStream {
                            sender_identity,
                            messages: vec![request],
                        },
                    );
                    None
                } else if let Some(stream) = streams.get_mut(&session_key) {
                    stream.messages.push(request);
                    None
                } else {
                    None
                }
            }
        };

        let Some((messages, response_identity)) = to_process else {
            return Ok(());
        };

        let service = messages
            .first()
            .and_then(|m| m.call_metadata.as_ref())
            .map(|m| m.service.as_str())
            .unwrap_or("");
        let method = messages
            .first()
            .and_then(|m| m.call_metadata.as_ref())
            .map(|m| m.method.as_str())
            .unwrap_or("");
        let request_end_of_stream = messages.iter().any(|r| r.end_of_stream);
        let rpc_messages: Vec<RpcMessage> = messages
            .iter()
            .map(|r| RpcMessage {
                payload: r.request_message.clone(),
                metadata: RequestMetadata {
                    sender_identity: r.sender_identity.clone(),
                },
            })
            .collect();
        let result = bridge.handle_messages(service, method, &rpc_messages).await;

        match result {
            Ok(body) => match body {
                ResponseBody::Complete(chunks) => {
                    let len = chunks.len();
                    log::info!(
                        "[echo_server] RPC request_id={} response sent ({} chunk(s))",
                        request_id,
                        len
                    );
                    for (i, bytes) in chunks.into_iter().enumerate() {
                        let end_of_stream = i == len - 1;
                        let response = crate::proto::RpcResponse {
                            request_id,
                            response_message: bytes,
                            metadata: None,
                            end_of_stream,
                            error: None,
                            trailers: None,
                        };
                        let encoded = encode_response(response)?;
                        let packet = DataPacket {
                            payload: encoded,
                            topic: Some(RPC_TOPIC.to_string()),
                            reliable: true,
                            destination_identities: vec![response_identity.clone()],
                        };
                        local
                            .publish_data(packet)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
                ResponseBody::Streaming(mut rx) => {
                    log::info!(
                        "[echo_server] RPC request_id={} streaming response (spawned task) response_identity={:?}",
                        request_id,
                        response_identity
                    );
                    let local = local.clone();
                    let response_identity = response_identity.clone();
                    let send_empty_end_frame = request_end_of_stream;
                    tokio::spawn(async move {
                        let mut chunk_index = 0u64;
                        while let Some(item) = rx.recv().await {
                            let bytes = match item {
                                Ok(b) => b,
                                Err(e) => {
                                    log::error!(
                                        "Stream request_id={} chunk error: {}",
                                        request_id,
                                        e
                                    );
                                    break;
                                }
                            };
                            log::info!(
                                "[echo_server] RPC request_id={} stream chunk #{} received ({} bytes)",
                                request_id,
                                chunk_index + 1,
                                bytes.len()
                            );
                            let response = crate::proto::RpcResponse {
                                request_id,
                                response_message: bytes,
                                metadata: None,
                                end_of_stream: false,
                                error: None,
                                trailers: None,
                            };
                            if let Ok(encoded) = encode_response(response) {
                                let len = encoded.len();
                                let packet = DataPacket {
                                    payload: encoded,
                                    topic: Some(RPC_TOPIC.to_string()),
                                    reliable: true,
                                    destination_identities: vec![response_identity.clone()],
                                };
                                if local.publish_data(packet).await.is_err() {
                                    log::error!(
                                        "[echo_server] RPC request_id={} publish_data failed",
                                        request_id
                                    );
                                    break;
                                }
                                log::info!(
                                    "[echo_server] RPC request_id={} published chunk #{} ({} bytes)",
                                    request_id,
                                    chunk_index + 1,
                                    len
                                );
                            }
                            chunk_index += 1;
                        }
                        if send_empty_end_frame {
                            let end_response = crate::proto::RpcResponse {
                                request_id,
                                response_message: vec![],
                                metadata: None,
                                end_of_stream: true,
                                error: None,
                                trailers: None,
                            };
                            if let Ok(encoded) = encode_response(end_response) {
                                let _ = local
                                    .publish_data(DataPacket {
                                        payload: encoded,
                                        topic: Some(RPC_TOPIC.to_string()),
                                        reliable: true,
                                        destination_identities: vec![response_identity],
                                    })
                                    .await;
                            }
                        }
                        log::info!(
                            "[echo_server] RPC request_id={} stream finished ({} chunks)",
                            request_id,
                            chunk_index
                        );
                    });
                }
            },
            Err(status) => {
                log::info!(
                    "[echo_server] RPC request_id={} error response: {}",
                    request_id,
                    status
                );
                let response = response_from_result(request_id, Err(status));
                let encoded = encode_response(response)?;
                let packet = DataPacket {
                    payload: encoded,
                    topic: Some(RPC_TOPIC.to_string()),
                    reliable: true,
                    destination_identities: vec![response_identity],
                };
                local
                    .publish_data(packet)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    fn spawn_streaming_response(
        request_id: i32,
        output: ResponseBody,
        local: LocalParticipant,
        response_identity: ParticipantIdentity,
        send_empty_end_frame: bool,
        shared_publisher: Option<SharedPublisher>,
    ) {
        match output {
            ResponseBody::Streaming(mut rx) => {
                log::info!(
                    "[echo_server] RPC request_id={} bidi streaming response (spawned task) response_identity={:?} reconnectable={}",
                    request_id,
                    response_identity,
                    shared_publisher.is_some(),
                );
                tokio::spawn(async move {
                    let mut chunk_index = 0u64;
                    while let Some(item) = rx.recv().await {
                        let bytes = match item {
                            Ok(b) => b,
                            Err(e) => {
                                log::error!("Stream request_id={} chunk error: {}", request_id, e);
                                break;
                            }
                        };
                        let response = crate::proto::RpcResponse {
                            request_id,
                            response_message: bytes,
                            metadata: None,
                            end_of_stream: false,
                            error: None,
                            trailers: None,
                        };
                        if let Ok(encoded) = encode_response(response) {
                            let dest = [response_identity.clone()];
                            if let Some(ref sp) = shared_publisher {
                                if let Err(e) = sp.publish_data(encoded, &dest).await {
                                    log::error!(
                                        "[echo_server] RPC request_id={} reconnectable publish failed: {}",
                                        request_id,
                                        e,
                                    );
                                    break;
                                }
                            } else {
                                let packet = DataPacket {
                                    payload: encoded,
                                    topic: Some(RPC_TOPIC.to_string()),
                                    reliable: true,
                                    destination_identities: dest.to_vec(),
                                };
                                if local.publish_data(packet).await.is_err() {
                                    log::error!(
                                        "[echo_server] RPC request_id={} publish_data failed",
                                        request_id
                                    );
                                    break;
                                }
                            }
                        }
                        chunk_index += 1;
                    }
                    if send_empty_end_frame {
                        let end_response = crate::proto::RpcResponse {
                            request_id,
                            response_message: vec![],
                            metadata: None,
                            end_of_stream: true,
                            error: None,
                            trailers: None,
                        };
                        if let Ok(encoded) = encode_response(end_response) {
                            let dest = [response_identity];
                            if let Some(ref sp) = shared_publisher {
                                let _ = sp.publish_data(encoded, &dest).await;
                            } else {
                                let _ = local
                                    .publish_data(DataPacket {
                                        payload: encoded,
                                        topic: Some(RPC_TOPIC.to_string()),
                                        reliable: true,
                                        destination_identities: dest.to_vec(),
                                    })
                                    .await;
                            }
                        }
                    }
                    log::info!(
                        "[echo_server] RPC request_id={} stream finished ({} chunks)",
                        request_id,
                        chunk_index
                    );
                });
            }
            ResponseBody::Complete(chunks) => {
                log::info!(
                    "[echo_server] RPC request_id={} bidi complete response ({} chunk(s))",
                    request_id,
                    chunks.len()
                );
                let len = chunks.len();
                tokio::spawn(async move {
                    for (i, bytes) in chunks.into_iter().enumerate() {
                        let end_of_stream = i == len - 1;
                        let response = crate::proto::RpcResponse {
                            request_id,
                            response_message: bytes,
                            metadata: None,
                            end_of_stream,
                            error: None,
                            trailers: None,
                        };
                        if let Ok(encoded) = encode_response(response) {
                            let packet = DataPacket {
                                payload: encoded,
                                topic: Some(RPC_TOPIC.to_string()),
                                reliable: true,
                                destination_identities: vec![response_identity.clone()],
                            };
                            let _ = local.publish_data(packet).await;
                        }
                    }
                });
            }
        }
    }

    /// Access the underlying room.
    pub fn room(&self) -> &Room {
        &self.room
    }
}

/// Resolve which participant identity to send responses to.
/// Prefers sender_identity from the request when present and non-empty; otherwise falls back to event participant or sole remote.
pub(crate) fn resolve_response_identity(
    request: &RpcRequest,
    event_participant: Option<ParticipantIdentity>,
    remote_identities: &[ParticipantIdentity],
) -> Option<ParticipantIdentity> {
    if let Some(ref s) = request.sender_identity {
        if !s.is_empty() {
            return Some(s.clone().into());
        }
    }
    event_participant.or_else(|| {
        if remote_identities.len() == 1 {
            Some(remote_identities[0].clone())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::CallMetadata;

    #[test]
    fn resolve_response_identity_uses_sender_identity_from_request_when_present() {
        let request = RpcRequest {
            request_id: 1,
            request_message: vec![],
            call_metadata: Some(CallMetadata {
                service: "test.EchoService".to_string(),
                method: "Echo".to_string(),
            }),
            metadata: None,
            end_of_stream: true,
            abort: false,
            sender_identity: Some("client1".to_string()),
        };
        let event_participant = Some("client2".to_string().into());
        let remote_identities = vec!["client1".to_string().into(), "client2".to_string().into()];
        let result = resolve_response_identity(&request, event_participant, &remote_identities);
        assert_eq!(
            result.as_ref().map(|p| p.as_str()),
            Some("client1"),
            "response must be sent to sender_identity from request, not event participant"
        );
    }
}
