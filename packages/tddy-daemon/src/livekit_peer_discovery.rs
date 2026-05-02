//! LiveKit `common_room` peer discovery: participant metadata advertisements, eligible daemon listing,
//! and **StartSession** routing via LiveKit data-channel RPC to peer daemons.
//!
//! # Advertisement JSON
//!
//! Published with [`livekit::prelude::LocalParticipant::set_metadata`]:
//! `{"instance_id":"<stable id>","label":"<human-readable>"}`.
//! When metadata is missing, the LiveKit participant **identity** string is used as `instance_id`.
//!
//! # Trust and security
//!
//! **Anyone who can join the configured LiveKit room** (same project URL, API key/secret, and
//! `livekit.common_room` name) can appear in **ListEligibleDaemons** and receive a forwarded
//! **StartSession** RPC. The forwarded request includes the client **`session_token`** and full
//! protobuf body. Treat the shared room as a **trusted peer group** (private LiveKit project,
//! network-restricted access); this is **not** a substitute for cryptographic proof that a
//! participant runs authentic `tddy-daemon` software.
//!
//! # Merge policy for eligible rows
//!
//! [`LiveKitEligibleDaemonSource::list_eligible_daemons`] calls [`merge_discovered_peers_ordered`].
//! If merge fails (e.g. empty local id), we **log a warning and return only the local daemon row**
//! so the UI and RPCs stay usable; operators should watch logs for repeated merge failures.
//!
//! # Discovery loop timing
//!
//! We call [`CommonRoomPeerRegistry::sync_from_room`] on participant connect/disconnect events and
//! also on a **500 ms** tick so transient SDK/event gaps do not leave stale peer rows indefinitely.
//! Reconnect backoff after a dropped room is **2 s** before retrying [`common_room_discovery_cycle`],
//! or **6 s** after [`DisconnectReason::DuplicateIdentity`] so the server can release the identity.
//!
//! # Operator logs (troubleshooting reconnect loops)
//!
//! With `dev.daemon.yaml`, daemon lines go to **`tmp/logs/daemon`**; LiveKit / WebRTC selectors also
//! write to **`tmp/logs/webrtc`**. Per-remote-participant metadata classification (**empty metadata**,
//! **not valid daemon advertisement**) uses log target
//! [`LOG_LIVEKIT_PEER_METADATA`] and is written to **`tmp/logs/daemon-livekit-peer-metadata`** when
//! that target is configured (see `dev.daemon.yaml`). Watch common-room disconnects and duplicate-identity fights:
//!
//! `grep -E 'common_room_discovery|LiveKit connected|LiveKit disconnected|OAuth tunnel follower: supervisor ended' tmp/logs/daemon`
//!
//! Rapid **`DuplicateIdentity`** means two processes share the same LiveKit identity in
//! `livekit.common_room` (often hostname, e.g. `udoo`). Use **`daemon_instance_id_append_startup_timestamp: true`**
//! (see `dev.desktop.yaml`) or stop the extra daemon.
//!
//! **`set_metadata`** (daemon advertisement JSON) can return **timeout** even when other clients already see
//! your metadata. In `livekit` **0.7.x**, [`LocalParticipant::set_metadata`](https://docs.rs/livekit/latest/livekit/prelude/struct.LocalParticipant.html#method.set_metadata)
//! waits up to **5 s per attempt** for a **RequestResponse** on the signal channel; we **retry** on the
//! **500 ms** registry tick (at most one SDK call every **5 s**) until
//! **`livekit.common_room_set_metadata_timeout_secs`** elapses per round (default **60**), so room
//! events still interleave. Your cached
//! [`LocalParticipant::metadata`](https://docs.rs/livekit/latest/livekit/prelude/struct.LocalParticipant.html#method.metadata)
//! can still update earlier via **ParticipantUpdate** from the server. Enable **DEBUG** for
//! `common_room_discovery: set_metadata` lines to compare cached metadata vs intent after each attempt.
//! Failures are **logged only**; the room stays connected and **`set_metadata` is retried every
//! **10 s** after a failed round (and again after **`RoomEvent::Reconnected`** if needed).
//!
//! # Forwarding and `RpcClient`
//!
//! Each [`forward_start_session_via_livekit`] call uses [`Room::subscribe`] and
//! [`RpcClient::new_shared`](tddy_livekit::RpcClient::new_shared), which **spawns a background task** to consume that
//! subscription until the receiver is dropped. Repeated forwards therefore add redundant handlers;
//! acceptable when forwards are rare. A future optimization is a **single** room-scoped dispatcher
//! or **per-peer cached** clients with explicit lifecycle (today we prioritize correctness and
//! simplicity).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use livekit::prelude::{
    ConnectionState, RemoteParticipant, Room, RoomError, RoomEvent, RoomOptions,
};
use livekit::DisconnectReason;
use prost::Message;
use serde::Deserialize;
use tddy_service::proto::connection::{StartSessionRequest, StartSessionResponse};

use crate::config::DaemonConfig;
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};

/// After `RoomEvent::Connected`, yield before the first `set_metadata` attempt.
const SET_METADATA_AFTER_CONNECTED_SETTLE_MS: u64 = 400;
/// After a full `set_metadata` publish round fails (budget exhausted), retry while the room stays connected.
const SET_METADATA_RETRY_INTERVAL_SECS: u64 = 10;
/// Minimum spacing between SDK `set_metadata` calls (matches livekit **REQUEST_TIMEOUT** per attempt).
const SET_METADATA_MIN_SDK_CALL_INTERVAL: Duration = Duration::from_secs(5);

/// Log target for classifying **remote** LiveKit participants (empty / non-advertisement metadata).
/// Configure `log.policies` in daemon YAML to send this target to a dedicated file.
pub const LOG_LIVEKIT_PEER_METADATA: &str = "tddy_daemon::livekit_peer_discovery::peer_metadata";

/// LiveKit-backed eligible listing plus the shared common-room [`Room`] handle for **StartSession** forwarding.
///
/// Construct this in `main` when `livekit.common_room` and credentials are set; pass [`None`] to
/// [`crate::connection_service::ConnectionServiceImpl::new`] for single-host / discovery-disabled mode.
pub struct LiveKitDiscoveryHandles {
    pub eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    pub common_room_livekit_room: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
}

/// Payload published by a daemon so peers can show it in **ListEligibleDaemons**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DaemonAdvertisement {
    pub instance_id: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
struct DaemonAdvertisementWire {
    instance_id: String,
    label: String,
}

/// Parse and normalize a daemon advertisement JSON string from the discovery transport.
pub fn parse_daemon_advertisement_json(input: &str) -> Result<DaemonAdvertisement, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty advertisement JSON".to_string());
    }
    let w: DaemonAdvertisementWire = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    let instance_id = w.instance_id.trim().to_string();
    let label = w.label.trim().to_string();
    if instance_id.is_empty() {
        return Err("advertisement instance_id is empty".to_string());
    }
    if label.is_empty() {
        return Err("advertisement label is empty".to_string());
    }
    Ok(DaemonAdvertisement { instance_id, label })
}

/// Merge the local row with discovered peers: **local first**, no duplicate `instance_id`, non-empty ids/labels.
pub fn merge_discovered_peers_ordered(
    local: EligibleDaemonInfo,
    remote: Vec<EligibleDaemonInfo>,
) -> Result<Vec<EligibleDaemonInfo>, String> {
    let local_id = local.instance_id.0.trim().to_string();
    if local_id.is_empty() {
        return Err("local instance_id is empty".to_string());
    }
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(local_id.clone());
    let mut out = vec![local];
    for r in remote {
        let id = r.instance_id.0.trim().to_string();
        if id.is_empty() || id == local_id {
            continue;
        }
        let label = r.label.trim().to_string();
        if label.is_empty() {
            continue;
        }
        if seen.insert(id.clone()) {
            out.push(EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id),
                label,
            });
        }
    }
    Ok(out)
}

/// Whether **StartSession** should run locally or be forwarded to a discovered peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartSessionPeerRoute {
    Local,
    Forward { peer_instance_id: String },
}

/// Decide routing from the requested `daemon_instance_id`, local id, and currently eligible peer ids.
pub fn classify_start_session_peer_route(
    local_instance_id: &str,
    requested_instance_id: &str,
    eligible_instance_ids: &[String],
) -> Result<StartSessionPeerRoute, String> {
    let req = requested_instance_id.trim();
    if req.is_empty() {
        log::debug!(
            "classify_start_session_peer_route: empty requested id → Local (local_instance_id={})",
            local_instance_id
        );
        return Ok(StartSessionPeerRoute::Local);
    }
    let local = local_instance_id.trim();
    if req == local {
        log::debug!("classify_start_session_peer_route: requested matches local → Local");
        return Ok(StartSessionPeerRoute::Local);
    }
    if eligible_instance_ids.iter().any(|id| id.trim() == req) {
        log::info!(
            "classify_start_session_peer_route: forwarding StartSession to peer instance_id={}",
            req
        );
        return Ok(StartSessionPeerRoute::Forward {
            peer_instance_id: req.to_string(),
        });
    }
    Err(format!(
        "unknown or not connected daemon_instance_id {:?}: peer is not in the current eligible daemon list (configure livekit.common_room and ensure the peer is in the same LiveKit room)",
        req
    ))
}

/// Registry of remote daemons observed in the shared common room (excludes the local row).
#[derive(Debug, Default)]
pub struct CommonRoomPeerRegistry {
    remotes: std::sync::RwLock<HashMap<String, EligibleDaemonInfo>>,
}

impl CommonRoomPeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace remote entries from a full room snapshot (authoritative for membership).
    pub fn sync_from_room(&self, room: &Room, local_instance_id: &str) {
        let mut next: HashMap<String, EligibleDaemonInfo> = HashMap::new();
        for (_, participant) in room.remote_participants() {
            if let Some(info) = remote_participant_to_eligible(participant, local_instance_id) {
                log::debug!(
                    target: LOG_LIVEKIT_PEER_METADATA,
                    "CommonRoomPeerRegistry: sync sees remote instance_id={} label_len={}",
                    info.instance_id.0,
                    info.label.len()
                );
                next.insert(info.instance_id.0.clone(), info);
            }
        }
        let n = next.len();
        {
            let mut g = self.remotes.write().expect("registry lock");
            *g = next;
        }
        log::info!(
            "CommonRoomPeerRegistry: synced {} remote daemon(s) from LiveKit room snapshot",
            n
        );
    }

    pub fn snapshot_remotes(&self) -> Vec<EligibleDaemonInfo> {
        self.remotes
            .read()
            .expect("registry lock")
            .values()
            .cloned()
            .collect()
    }

    /// Drop all remote rows (e.g. when discovery disconnects).
    pub fn clear(&self) {
        let mut g = self.remotes.write().expect("registry lock");
        g.clear();
        log::debug!("CommonRoomPeerRegistry: cleared all remote daemon entries");
    }
}

fn remote_participant_to_eligible(
    remote: RemoteParticipant,
    local_instance_id: &str,
) -> Option<EligibleDaemonInfo> {
    let identity_str = remote.identity().to_string();
    let meta = remote.metadata();
    let meta_trim = meta.trim();
    let (instance_id, label) = if !meta_trim.is_empty() {
        match parse_daemon_advertisement_json(meta_trim) {
            Ok(a) => (a.instance_id, a.label),
            Err(e) => {
                log::debug!(
                    target: LOG_LIVEKIT_PEER_METADATA,
                    "peer {} metadata not valid daemon advertisement ({}), falling back to identity",
                    identity_str,
                    e
                );
                (
                    identity_str.clone(),
                    format!("{identity_str} (LiveKit peer)"),
                )
            }
        }
    } else {
        log::debug!(
            target: LOG_LIVEKIT_PEER_METADATA,
            "peer {} has empty metadata; using LiveKit identity as instance_id",
            identity_str
        );
        (
            identity_str.clone(),
            format!("{identity_str} (LiveKit peer)"),
        )
    };
    let instance_id = instance_id.trim().to_string();
    if instance_id.is_empty() || instance_id == local_instance_id.trim() {
        return None;
    }
    let label = if label.trim().is_empty() {
        format!("{instance_id} (LiveKit peer)")
    } else {
        label.trim().to_string()
    };
    Some(EligibleDaemonInfo {
        instance_id: DaemonInstanceId(instance_id),
        label,
    })
}

fn process_startup_unix_ms_suffix() -> &'static str {
    static SUFFIX: OnceLock<String> = OnceLock::new();
    SUFFIX
        .get_or_init(|| {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("{ms}")
        })
        .as_str()
}

/// Resolved local daemon instance id string (config override or hostname default).
pub fn local_instance_id_for_config(config: &DaemonConfig) -> String {
    let base = config
        .daemon_instance_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::multi_host::local_daemon_instance_id().0);
    if config.daemon_instance_id_append_startup_timestamp {
        format!("{}-{}", base, process_startup_unix_ms_suffix())
    } else {
        base
    }
}

/// LiveKit-backed **EligibleDaemonSource** — reads [`CommonRoomPeerRegistry`] populated by the discovery task.
pub struct LiveKitEligibleDaemonSource {
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
}

impl LiveKitEligibleDaemonSource {
    pub fn new(config: Arc<DaemonConfig>, registry: Arc<CommonRoomPeerRegistry>) -> Self {
        Self { config, registry }
    }

    fn local_row(&self) -> EligibleDaemonInfo {
        let id = local_instance_id_for_config(&self.config);
        let label = format!("{id} (this daemon)");
        EligibleDaemonInfo {
            instance_id: DaemonInstanceId(id),
            label,
        }
    }
}

impl EligibleDaemonSource for LiveKitEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        let local = self.local_row();
        let remotes = self.registry.snapshot_remotes();
        log::debug!(
            "LiveKitEligibleDaemonSource::list_eligible_daemons: local_id={} remote_count={}",
            local.instance_id.0,
            remotes.len()
        );
        // Deliberate degradation: see module docs ("Merge policy for eligible rows").
        merge_discovered_peers_ordered(local, remotes).unwrap_or_else(|e| {
            log::warn!(
                "merge_discovered_peers_ordered failed: {} — returning local only",
                e
            );
            vec![self.local_row()]
        })
    }
}

/// Spawn a background task that joins `livekit.common_room`, publishes metadata, and keeps
/// [`CommonRoomPeerRegistry`] in sync. Also stores [`Room`] in `room_slot` for **StartSession** forwarding.
pub fn spawn_common_room_discovery_task(
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
) {
    if config.codex_oauth_loopback_proxy_eligible {
        let room_slot_oauth = room_slot.clone();
        tokio::spawn(async move {
            crate::oauth_loopback_tunnel::run_oauth_tunnel_supervisor_follow_room_slot(
                room_slot_oauth,
            )
            .await;
        });
    } else {
        log::info!(
            target: "tddy_daemon::oauth_tunnel",
            "OAuth loopback TCP proxy disabled (codex_oauth_loopback_proxy_eligible=false); no bind on 127.0.0.1 callback ports from this process"
        );
    }
    tokio::spawn(async move {
        loop {
            let local_id = local_instance_id_for_config(&config);
            let outcome =
                common_room_discovery_cycle(config.clone(), registry.clone(), room_slot.clone())
                    .await;
            let retry_secs: u64 = match &outcome {
                Ok(Some(DisconnectReason::DuplicateIdentity)) => 6,
                _ => 2,
            };
            if let Err(e) = &outcome {
                log::warn!(
                    "common_room_discovery_cycle ended: {e:#} — clearing room handle; retry in {}s (local_livekit_identity={})",
                    retry_secs,
                    local_id
                );
            }
            {
                let mut g = room_slot.write().await;
                *g = None;
            }
            registry.clear();
            tokio::time::sleep(Duration::from_secs(retry_secs)).await;
        }
    });
}

/// Validates `livekit` + `common_room` URL/key/secret strings for discovery.
fn livekit_common_room_connect_strings(
    config: &DaemonConfig,
) -> anyhow::Result<(String, String, String, String)> {
    let livekit = config
        .livekit
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("LiveKit not configured"))?;
    let room_name = livekit
        .common_room
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("livekit.common_room not set"))?
        .to_string();
    let url = livekit
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("livekit.url not set"))?
        .to_string();
    let api_key = livekit
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("livekit.api_key not set"))?
        .to_string();
    let api_secret = livekit
        .api_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("livekit.api_secret not set"))?
        .to_string();
    Ok((room_name, url, api_key, api_secret))
}

/// Connects to the common room and waits for [`RoomEvent::Connected`].
///
/// Daemon advertisement metadata is published from [`run_common_room_registry_loop`] (retries on failure).
/// Earlier room events are buffered and replayed into that loop.
async fn connect_common_room_publish_metadata(
    room_name: &str,
    url: &str,
    token: &str,
    local_id: &str,
) -> anyhow::Result<(
    Arc<Room>,
    tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    VecDeque<RoomEvent>,
    String,
)> {
    let (room, mut events) = Room::connect(url, token, RoomOptions::default()).await?;
    let room = Arc::new(room);
    let lp = room.local_participant();
    log::info!(
        "common_room_discovery: LiveKit connected room={} identity={} participant_sid={:?} connection_state={:?}",
        room_name,
        local_id,
        lp.sid(),
        room.connection_state()
    );
    let adv = DaemonAdvertisement {
        instance_id: local_id.to_string(),
        label: format!("{local_id} (this daemon)"),
    };
    let meta_json = serde_json::to_string(&adv)?;
    let meta_len = meta_json.len();

    let mut buffered = VecDeque::new();
    log::info!(
        "common_room_discovery: awaiting RoomEvent::Connected (metadata_len={} will publish in session loop)",
        meta_len
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = events
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("event channel closed before RoomEvent::Connected"))?;
            match ev {
                RoomEvent::Connected {
                    participants_with_tracks,
                } => {
                    log::info!(
                        "common_room_discovery: RoomEvent::Connected remote_participants_with_tracks={}",
                        participants_with_tracks.len()
                    );
                    break;
                }
                other => buffered.push_back(other),
            }
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("timeout waiting for RoomEvent::Connected (30s)"))??;

    Ok((room, events, buffered, meta_json))
}

/// One [`LocalParticipant::set_metadata`] call with DEBUG context (SDK **5 s** timeout per call).
async fn set_daemon_advertisement_metadata_once(
    room: &Room,
    phase: &'static str,
    attempt: u32,
    intent: String,
) -> Result<(), RoomError> {
    let lp = room.local_participant();
    let cached_before = lp.metadata();
    log::debug!(
        "common_room_discovery: set_metadata attempt phase={phase} sdk_attempt={} connection_state={:?} participant_sid={:?} intent_len={} cached_len={} cached_eq_intent={}",
        attempt,
        room.connection_state(),
        lp.sid(),
        intent.len(),
        cached_before.len(),
        cached_before == intent,
    );
    let t0 = Instant::now();
    let out = lp.set_metadata(intent.clone()).await;
    let elapsed_ms = t0.elapsed().as_millis();
    let cached_after = room.local_participant().metadata();
    match &out {
        Ok(()) => {
            log::debug!(
                "common_room_discovery: set_metadata ok phase={phase} sdk_attempt={} elapsed_ms={} cached_len_after={}",
                attempt,
                elapsed_ms,
                cached_after.len(),
            );
        }
        Err(e) => {
            let after_matches = cached_after == intent;
            log::debug!(
                "common_room_discovery: set_metadata err phase={phase} sdk_attempt={} elapsed_ms={} err={e:?} cached_len_after={} cached_eq_intent_after_err={}",
                attempt,
                elapsed_ms,
                cached_after.len(),
                after_matches,
            );
            log::debug!(
                "common_room_discovery: set_metadata err hint phase={phase} sdk_attempt={}: {}",
                attempt,
                if after_matches {
                    "local participant cache already matches intent — server likely applied metadata (e.g. via ParticipantUpdate) but RequestResponse did not arrive within the SDK 5s timeout; safe to ignore if peers see the ad"
                } else {
                    "local cache still differs from intent — signaling ack likely missing and state not updated yet"
                },
            );
        }
    }
    out
}

#[derive(Debug)]
struct DaemonAdvPublishState {
    on_server: bool,
    /// Wall-clock end of the current publish round (`None` when published or between scheduled retries).
    round_deadline: Option<Instant>,
    last_sdk_call: Option<Instant>,
    sdk_attempt: u32,
}

/// One step of daemon-advertisement publish: registry sync is done by the caller.
async fn advance_daemon_adv_publish_on_tick(
    room: &Room,
    local_id: &str,
    intent: &str,
    budget: Duration,
    st: &mut DaemonAdvPublishState,
) {
    if st.on_server {
        return;
    }
    if room.local_participant().metadata() == intent {
        log::info!(
            "common_room_discovery: published daemon advertisement for instance_id={} (local cache matched intent)",
            local_id
        );
        st.on_server = true;
        st.round_deadline = None;
        return;
    }
    let Some(deadline) = st.round_deadline else {
        return;
    };
    let now = Instant::now();
    if now >= deadline {
        log::warn!(
            "common_room_discovery: set_metadata publish round timed out (budget {}s; room stays connected; retry every {}s)",
            budget.as_secs(),
            SET_METADATA_RETRY_INTERVAL_SECS
        );
        st.round_deadline = None;
        return;
    }
    let allow_publish_sdk_call = match st.last_sdk_call {
        None => true,
        Some(t) => now.saturating_duration_since(t) >= SET_METADATA_MIN_SDK_CALL_INTERVAL,
    };
    if !allow_publish_sdk_call {
        return;
    }
    st.sdk_attempt = st.sdk_attempt.saturating_add(1);
    st.last_sdk_call = Some(now);
    match set_daemon_advertisement_metadata_once(
        room,
        "publish",
        st.sdk_attempt,
        intent.to_string(),
    )
    .await
    {
        Ok(()) => {
            log::info!(
                "common_room_discovery: published daemon advertisement for instance_id={}",
                local_id
            );
            st.on_server = true;
            st.round_deadline = None;
        }
        Err(e) => {
            if room.local_participant().metadata() == intent {
                log::info!(
                    "common_room_discovery: published daemon advertisement for instance_id={} (local cache matched after SDK error)",
                    local_id
                );
                st.on_server = true;
                st.round_deadline = None;
            } else if Instant::now() >= deadline {
                log::warn!(
                    "common_room_discovery: set_metadata publish round ended after budget (last err: {e:#}; retry every {}s)",
                    SET_METADATA_RETRY_INTERVAL_SECS
                );
                st.round_deadline = None;
            }
        }
    }
}

/// Runs the periodic + event-driven registry sync until the room disconnects or the event channel ends.
///
/// Returns `Some(reason)` after `RoomEvent::Disconnected`, or `None` when the event channel
/// ends. Always calls `Room::close` so the next discovery cycle starts from a clean engine state.
#[allow(clippy::too_many_arguments)]
async fn run_common_room_registry_loop(
    room: Arc<Room>,
    mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    mut event_buffer: VecDeque<RoomEvent>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_name: String,
    local_id: String,
    daemon_adv_metadata: String,
    set_metadata_budget: Duration,
) -> Option<DisconnectReason> {
    registry.sync_from_room(room.as_ref(), &local_id);

    tokio::time::sleep(Duration::from_millis(
        SET_METADATA_AFTER_CONNECTED_SETTLE_MS,
    ))
    .await;

    let mut publish_st = DaemonAdvPublishState {
        on_server: room.local_participant().metadata() == daemon_adv_metadata,
        round_deadline: None,
        last_sdk_call: None,
        sdk_attempt: 0,
    };
    if publish_st.on_server {
        log::info!(
            "common_room_discovery: daemon advertisement already in local participant metadata for instance_id={}",
            local_id
        );
    } else {
        publish_st.round_deadline = Some(Instant::now() + set_metadata_budget);
        log::debug!(
            "common_room_discovery: set_metadata publish round started budget_ms={}",
            set_metadata_budget.as_millis()
        );
    }

    let mut meta_tick =
        tokio::time::interval(Duration::from_secs(SET_METADATA_RETRY_INTERVAL_SECS));
    meta_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    meta_tick.tick().await;

    // 500 ms: safety net if participant events are delayed or missed; see module docs.
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _ = async {
                tick.tick().await;
                registry.sync_from_room(room.as_ref(), &local_id);
                advance_daemon_adv_publish_on_tick(
                    room.as_ref(),
                    &local_id,
                    &daemon_adv_metadata,
                    set_metadata_budget,
                    &mut publish_st,
                )
                .await;
            } => {}
            _ = meta_tick.tick(), if !publish_st.on_server => {
                publish_st.round_deadline = Some(Instant::now() + set_metadata_budget);
                publish_st.last_sdk_call = None;
                log::debug!(
                    "common_room_discovery: set_metadata scheduled retry round budget_ms={}",
                    set_metadata_budget.as_millis()
                );
            }
            ev = async {
                if let Some(e) = event_buffer.pop_front() {
                    Some(e)
                } else {
                    events.recv().await
                }
            } => {
                let Some(ev) = ev else {
                    log::warn!(
                        "common_room_discovery: LiveKit disconnected room={} identity={} (event channel closed)",
                        room_name,
                        local_id
                    );
                    let _ = room.close().await;
                    return None;
                };
                match ev {
                    RoomEvent::Connected {
                        participants_with_tracks,
                    } => {
                        log::info!(
                            "common_room_discovery: LiveKit session active room={} identity={} remote_participants_with_tracks={}",
                            room_name,
                            local_id,
                            participants_with_tracks.len()
                        );
                    }
                    RoomEvent::Reconnecting => {
                        log::info!(
                            "common_room_discovery: LiveKit reconnecting room={} identity={}",
                            room_name,
                            local_id
                        );
                    }
                    RoomEvent::Reconnected => {
                        log::info!(
                            "common_room_discovery: LiveKit reconnected room={} identity={}",
                            room_name,
                            local_id
                        );
                        publish_st.on_server = false;
                        publish_st.round_deadline = Some(Instant::now() + set_metadata_budget);
                        publish_st.last_sdk_call = None;
                        publish_st.sdk_attempt = 0;
                    }
                    RoomEvent::ParticipantConnected(p) => {
                        log::info!(
                            "common_room_discovery: ParticipantConnected identity={:?}",
                            p.identity()
                        );
                        registry.sync_from_room(room.as_ref(), &local_id);
                    }
                    RoomEvent::ParticipantDisconnected(p) => {
                        log::info!(
                            "common_room_discovery: ParticipantDisconnected identity={:?} reason={:?}",
                            p.identity(),
                            p.disconnect_reason()
                        );
                        registry.sync_from_room(room.as_ref(), &local_id);
                    }
                    RoomEvent::ConnectionStateChanged(state) => {
                        if state != ConnectionState::Connected {
                            log::info!(
                                "common_room_discovery: LiveKit connection state {:?} room={} identity={}",
                                state,
                                room_name,
                                local_id
                            );
                        }
                    }
                    RoomEvent::Disconnected { reason } => {
                        if reason == DisconnectReason::DuplicateIdentity {
                            log::warn!(
                                "common_room_discovery: LiveKit disconnected room={} identity={} reason=DuplicateIdentity — another client joined livekit.common_room with the same identity; stop the other process or set daemon_instance_id / daemon_instance_id_append_startup_timestamp (see dev.desktop.yaml)",
                                room_name,
                                local_id
                            );
                        } else {
                            log::warn!(
                                "common_room_discovery: LiveKit disconnected room={} identity={} reason={:?}",
                                room_name,
                                local_id,
                                reason
                            );
                        }
                        let _ = room.close().await;
                        return Some(reason);
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn common_room_discovery_cycle(
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
) -> anyhow::Result<Option<DisconnectReason>> {
    let (room_name, url, api_key, api_secret) = livekit_common_room_connect_strings(&config)?;
    let set_metadata_budget = config.common_room_set_metadata_attempt_budget();
    let local_id = local_instance_id_for_config(&config);
    let gen = tddy_livekit::TokenGenerator::new(
        api_key,
        api_secret,
        room_name.clone(),
        local_id.clone(),
        Duration::from_secs(3600),
    );
    let token = gen
        .generate()
        .map_err(|e| anyhow::anyhow!("LiveKit token: {e}"))?;
    log::info!(
        "common_room_discovery: connecting to LiveKit room={} identity={} url_len={}",
        room_name,
        local_id,
        url.len()
    );
    let (room, events, event_buffer, daemon_adv_metadata) =
        connect_common_room_publish_metadata(&room_name, &url, &token, &local_id).await?;
    {
        let mut g = room_slot.write().await;
        *g = Some(room.clone());
    }
    log::info!(
        "common_room_discovery: common-room LiveKit session ready for instance_id={} (metadata publish is best-effort)",
        local_id
    );

    let end = run_common_room_registry_loop(
        room,
        events,
        event_buffer,
        registry,
        room_name,
        local_id,
        daemon_adv_metadata,
        set_metadata_budget,
    )
    .await;
    Ok(end)
}

/// Forward **StartSession** to another daemon in the common room via LiveKit data-channel RPC.
///
/// See module docs: each call subscribes to room events and spawns a [`tddy_livekit::RpcClient`] background loop.
pub async fn forward_start_session_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<StartSessionResponse, tddy_rpc::Status> {
    let room_arc = {
        let g = room_slot.read().await;
        g.clone()
    }
    .ok_or_else(|| {
        tddy_rpc::Status::failed_precondition(
            "LiveKit common room is not connected on this daemon; cannot forward StartSession to a peer",
        )
    })?;
    log::debug!(
        "forward_start_session_via_livekit: peer_instance_id={} (new Room::subscribe + RpcClient)",
        peer_instance_id
    );
    let rpc_events = room_arc.subscribe();
    let client =
        tddy_livekit::RpcClient::new_shared(room_arc, peer_instance_id.to_string(), rpc_events);
    let body = request.encode_to_vec();
    let out = client
        .call_unary("connection.ConnectionService", "StartSession", body)
        .await?;
    StartSessionResponse::decode(out.as_slice())
        .map_err(|e| tddy_rpc::Status::internal(format!("decode StartSessionResponse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_host::DaemonInstanceId;

    #[test]
    fn parse_daemon_advertisement_accepts_documented_json_shape() {
        let json = r#"{"instance_id":"peer-a","label":"Peer A"}"#;
        let got =
            parse_daemon_advertisement_json(json).expect("parse documented advertisement JSON");
        assert_eq!(got.instance_id, "peer-a");
        assert_eq!(got.label, "Peer A");
    }

    #[test]
    fn merge_discovered_peers_ordered_places_local_first() {
        let local = EligibleDaemonInfo {
            instance_id: DaemonInstanceId("local-host".to_string()),
            label: "This machine".to_string(),
        };
        let remote = vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId("remote-1".to_string()),
            label: "Remote".to_string(),
        }];
        let merged = merge_discovered_peers_ordered(local.clone(), remote).expect("merge");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].instance_id, local.instance_id);
    }

    #[test]
    fn merge_discovered_peers_deduplicates_remote_by_instance_id() {
        let local = EligibleDaemonInfo {
            instance_id: DaemonInstanceId("local-host".to_string()),
            label: "Local".to_string(),
        };
        let remote = vec![
            EligibleDaemonInfo {
                instance_id: DaemonInstanceId("dup".to_string()),
                label: "First".to_string(),
            },
            EligibleDaemonInfo {
                instance_id: DaemonInstanceId("dup".to_string()),
                label: "Second".to_string(),
            },
        ];
        let merged = merge_discovered_peers_ordered(local, remote).expect("merge");
        assert_eq!(
            merged.iter().filter(|e| e.instance_id.0 == "dup").count(),
            1
        );
    }

    #[test]
    fn livekit_eligible_daemon_source_lists_at_least_local_row() {
        let config = Arc::new(DaemonConfig::default());
        let registry = Arc::new(CommonRoomPeerRegistry::new());
        let src = LiveKitEligibleDaemonSource::new(config, registry);
        let list = src.list_eligible_daemons();
        assert!(
            !list.is_empty(),
            "LiveKit-backed source must list the local daemon once discovery is wired"
        );
    }

    #[test]
    fn classify_start_session_peer_route_treats_empty_request_as_local() {
        let route = classify_start_session_peer_route("local", "", &[]).expect("classify");
        assert_eq!(route, StartSessionPeerRoute::Local);
    }

    #[test]
    fn classify_start_session_peer_route_forwards_when_peer_is_eligible() {
        let route =
            classify_start_session_peer_route("local", "remote-peer", &["remote-peer".to_string()])
                .expect("classify");
        assert_eq!(
            route,
            StartSessionPeerRoute::Forward {
                peer_instance_id: "remote-peer".to_string(),
            }
        );
    }

    #[test]
    fn local_instance_id_appends_stable_timestamp_suffix_when_configured() {
        let cfg = DaemonConfig {
            daemon_instance_id: Some("my-daemon".to_string()),
            daemon_instance_id_append_startup_timestamp: true,
            ..Default::default()
        };
        let a = local_instance_id_for_config(&cfg);
        let b = local_instance_id_for_config(&cfg);
        assert_eq!(a, b);
        let suffix = a.strip_prefix("my-daemon-").expect("prefix");
        assert!(suffix.chars().all(|c| c.is_ascii_digit()));
    }
}
