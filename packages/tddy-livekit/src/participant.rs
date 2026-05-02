//! LiveKit room participant that serves RPC over the data channel.

use livekit::prelude::*;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex};

use tddy_rpc::{RequestMetadata, RpcMessage};

use crate::bridge::{ResponseBody, RpcBridge};
use crate::envelope::{decode_request, encode_response, response_from_result};
use crate::projects_registry;
use crate::proto::{CallMetadata, RpcRequest};
use crate::rpc_trace;
use crate::token::TokenGenerator;

const RPC_TOPIC: &str = "tddy-rpc";

/// Canonical JSON key for the daemon project registry row count published on server LiveKit participants.
pub const OWNED_PROJECT_COUNT_METADATA_KEY: &str = "owned_project_count";

/// Returns the number of project rows under `path` using the same `projects.yaml` layout as **tddy-daemon**
/// (`project_storage`; see [`crate::projects_registry`] — kept in sync because this crate cannot depend on the daemon).
pub fn owned_project_count_for_projects_dir(path: &Path) -> anyhow::Result<u64> {
    log::debug!(
        target: "tddy_livekit::metadata",
        "owned_project_count_for_projects_dir: dir={}",
        path.display()
    );
    projects_registry::owned_project_row_count(path)
}

/// Shallow-merge two JSON **objects** for [`LocalParticipant::set_metadata`].
///
/// Top-level keys from `update` overwrite or add to `baseline`. Nested values are replaced as a whole (not deep-merged).
/// Non-object `baseline` (or invalid JSON) is treated as an empty object with a warning.
pub fn merge_participant_metadata_json(
    baseline: &str,
    update: &str,
) -> Result<String, serde_json::Error> {
    log::debug!(
        target: "tddy_livekit::metadata",
        "merge_participant_metadata_json: baseline_len={} update_len={}",
        baseline.len(),
        update.len()
    );

    let mut base_map = if baseline.trim().is_empty() {
        serde_json::Map::new()
    } else {
        match serde_json::from_str::<Value>(baseline)? {
            Value::Object(m) => m,
            other => {
                log::warn!(
                    target: "tddy_livekit::metadata",
                    "merge_participant_metadata_json: baseline is not a JSON object (got {:?}); starting from {{}}",
                    other
                );
                serde_json::Map::new()
            }
        }
    };

    let update_val: Value = if update.trim().is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(update)?
    };

    if let Value::Object(up_map) = update_val {
        for (k, v) in up_map {
            base_map.insert(k, v);
        }
    }

    let merged = Value::Object(base_map);
    log::debug!(
        target: "tddy_livekit::metadata",
        "merge_participant_metadata_json: merged top-level keys={}",
        merged.as_object().map(|m| m.len()).unwrap_or(0)
    );
    serde_json::to_string(&merged)
}

/// Applies [`watch::Receiver`] updates to LiveKit participant metadata via [`LocalParticipant::set_metadata`],
/// shallow-merging each payload into the current wire metadata so other keys (e.g. [`OWNED_PROJECT_COUNT_METADATA_KEY`]) stay intact.
///
/// `metadata_publish_lock` must be the same mutex used by other metadata publishers on this participant (Codex OAuth poller, project count).
pub fn spawn_local_participant_metadata_watcher(
    mut rx: watch::Receiver<String>,
    local: LocalParticipant,
    metadata_publish_lock: Arc<Mutex<()>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() {
                log::debug!(target: "tddy_livekit::metadata", "metadata watcher: channel closed");
                break;
            }
            let v = rx.borrow().clone();
            if v.is_empty() {
                continue;
            }
            let _guard = metadata_publish_lock.lock().await;
            let baseline = local.metadata();
            let merged = match merge_participant_metadata_json(&baseline, &v) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!(
                        target: "tddy_livekit::metadata",
                        "metadata watcher: merge failed: {}",
                        e
                    );
                    continue;
                }
            };
            log::info!(
                target: "tddy_livekit::metadata",
                "metadata watcher: applying merged metadata (len={})",
                merged.len()
            );
            if let Err(e) = local.set_metadata(merged).await {
                log::warn!("LiveKit set_metadata failed: {}", e);
            }
        }
    })
}

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
    /// Poll this path for an `https://` authorize URL (Codex `BROWSER` hook) and publish **metadata
    /// only** for UIs. Includes `callback_port` / `state` when
    /// derivable from the URL so the desktop relay matches the terminal-driven metadata shape.
    codex_oauth_watch: Option<PathBuf>,
    /// When set, publish [`OWNED_PROJECT_COUNT_METADATA_KEY`] from the registry at this directory (e.g. daemon project store).
    projects_registry_dir: Option<PathBuf>,
    /// Coordinates read–merge–write so OAuth, project count, and watch-channel updates do not clobber each other.
    metadata_publish_lock: Arc<Mutex<()>>,
}

impl<S: crate::bridge::RpcService> LiveKitParticipant<S> {
    /// Connect to a LiveKit room and create a participant that will serve RPC.
    pub async fn connect(
        url: &str,
        token: &str,
        service: S,
        room_options: RoomOptions,
        codex_oauth_watch: Option<PathBuf>,
        projects_registry_dir: Option<PathBuf>,
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
            codex_oauth_watch,
            projects_registry_dir,
            metadata_publish_lock: Arc::new(Mutex::new(())),
        })
    }

    /// Connect to a LiveKit room, sharing bidi session state and publisher across
    /// reconnection cycles. The SharedPublisher is updated with the new LocalParticipant
    /// so that output tasks from previous cycles can publish through the new room.
    #[allow(clippy::too_many_arguments)] // Reconnect path threads many handles; struct refactor is churn.
    async fn connect_for_reconnect(
        url: &str,
        token: &str,
        bridge: Arc<RpcBridge<S>>,
        room_options: RoomOptions,
        active_bidi: Arc<Mutex<HashMap<SessionKey, BidiStreamMeta>>>,
        active_bidi_sessions: Arc<Mutex<HashMap<SessionKey, BidiSession>>>,
        shared_publisher: SharedPublisher,
        codex_oauth_watch: Option<PathBuf>,
        projects_registry_dir: Option<PathBuf>,
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
            codex_oauth_watch,
            projects_registry_dir,
            metadata_publish_lock: Arc::new(Mutex::new(())),
        })
    }

    async fn apply_owned_project_count_to_local_metadata(
        local: &LocalParticipant,
        projects_dir: &Path,
        metadata_publish_lock: &Arc<Mutex<()>>,
    ) -> anyhow::Result<()> {
        let _guard = metadata_publish_lock.lock().await;
        let baseline = local.metadata();
        let count = owned_project_count_for_projects_dir(projects_dir)?;
        log::debug!(
            target: "tddy_livekit::metadata",
            "apply_owned_project_count: dir={} count={} baseline_len={}",
            projects_dir.display(),
            count,
            baseline.len()
        );
        let update = serde_json::json!({ OWNED_PROJECT_COUNT_METADATA_KEY: count }).to_string();
        let merged = merge_participant_metadata_json(&baseline, &update)
            .map_err(|e| anyhow::anyhow!("merge participant metadata: {}", e))?;
        local
            .set_metadata(merged)
            .await
            .map_err(|e| anyhow::anyhow!("set_metadata: {}", e))?;
        log::info!(
            target: "tddy_livekit::metadata",
            "published {}={} (merged with existing metadata)",
            OWNED_PROJECT_COUNT_METADATA_KEY,
            count
        );
        Ok(())
    }

    /// Connect with a JWT from `token_generator`, then run until the room disconnects or
    /// `shutdown` is set. Does not proactively reconnect for JWT rotation; the LiveKit SDK
    /// handles connection health and server-driven token refresh on the signal channel.
    pub async fn run_with_reconnect(
        url: &str,
        token_generator: &TokenGenerator,
        service: S,
        room_options: RoomOptions,
        shutdown: Arc<AtomicBool>,
        codex_oauth_watch: Option<PathBuf>,
        projects_registry_dir: Option<PathBuf>,
    ) {
        Self::run_with_reconnect_metadata(
            url,
            token_generator,
            service,
            room_options,
            shutdown,
            None,
            codex_oauth_watch,
            projects_registry_dir,
        )
        .await;
    }

    /// Like [`Self::run_with_reconnect`], but pushes `metadata_watch` values to the local participant metadata.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_reconnect_metadata(
        url: &str,
        token_generator: &TokenGenerator,
        service: S,
        room_options: RoomOptions,
        shutdown: Arc<AtomicBool>,
        metadata_watch: Option<watch::Receiver<String>>,
        codex_oauth_watch: Option<PathBuf>,
        projects_registry_dir: Option<PathBuf>,
    ) {
        let bridge = Arc::new(RpcBridge::new(service));
        let shared_publisher = SharedPublisher::new();
        let active_bidi: Arc<Mutex<HashMap<SessionKey, BidiStreamMeta>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let active_bidi_sessions: Arc<Mutex<HashMap<SessionKey, BidiSession>>> =
            Arc::new(Mutex::new(HashMap::new()));

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
            codex_oauth_watch.clone(),
            projects_registry_dir.clone(),
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

        let metadata_task = if let Some(rx) = metadata_watch {
            let local = participant.room().local_participant().clone();
            let lock = participant.metadata_publish_lock.clone();
            Some(spawn_local_participant_metadata_watcher(rx, local, lock))
        } else {
            None
        };

        log::info!(
            "[livekit] participant running (jwt_ttl={:?}, no timer-driven reconnect)",
            token_generator.ttl()
        );

        let shutdown_clone = shutdown.clone();
        tokio::select! {
            _ = participant.run() => {
                log::info!("[livekit] participant.run() returned (disconnected)");
            }
            _ = async {
                while !shutdown_clone.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => {
                log::info!("[livekit] shutdown requested");
            }
        }

        if let Some(t) = metadata_task {
            t.abort();
        }
    }

    /// Run the participant event loop. Processes DataReceived events for topic "tddy-rpc"
    /// and dispatches to the RpcBridge. Returns when the room disconnects.
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
                    let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());

                    // Must not `spawn` per packet: bidi input is forwarded with `input_tx.send` inside
                    // `handle_incoming`. Concurrent tasks can complete sends out of arrival order,
                    // reordering keystrokes on `StreamTerminalIO` (and any other bidi stream).
                    if let Err(e) = Self::handle_incoming(
                        &payload,
                        event_participant,
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
                        log::error!("RPC handle error: {}", e);
                    }
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

    /// Lock shared with [`spawn_local_participant_metadata_watcher`] and internal metadata publishers; pass to the watcher when wiring manually.
    pub fn metadata_publish_lock(&self) -> Arc<Mutex<()>> {
        self.metadata_publish_lock.clone()
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
    use serde_json::Value;

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

    #[test]
    fn merge_participant_metadata_json_retains_baseline_keys_on_partial_update() {
        let baseline = r#"{"codex_oauth":{"pending":false},"legacy":1}"#;
        let update = format!(r#"{{"{key}":9}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);
        let merged = merge_participant_metadata_json(baseline, &update).expect("merge");
        let v: Value = serde_json::from_str(&merged).expect("merged JSON");
        assert!(
            v.get("legacy").is_some(),
            "baseline-only keys must remain after merge; got {merged}"
        );
        assert_eq!(
            v.get(OWNED_PROJECT_COUNT_METADATA_KEY)
                .and_then(|x| x.as_u64()),
            Some(9)
        );
    }
}
