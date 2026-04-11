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
//! Reconnect backoff after a dropped room is **2 s** before retrying [`common_room_discovery_cycle`].
//!
//! # Forwarding and `RpcClient`
//!
//! Each [`forward_start_session_via_livekit`] call uses [`Room::subscribe`] and
//! [`RpcClient::new_shared`](tddy_livekit::RpcClient::new_shared), which **spawns a background task** to consume that
//! subscription until the receiver is dropped. Repeated forwards therefore add redundant handlers;
//! acceptable when forwards are rare. A future optimization is a **single** room-scoped dispatcher
//! or **per-peer cached** clients with explicit lifecycle (today we prioritize correctness and
//! simplicity).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{RemoteParticipant, Room, RoomEvent, RoomOptions};
use prost::Message;
use serde::Deserialize;
use tddy_service::proto::connection::{StartSessionRequest, StartSessionResponse};

use crate::config::DaemonConfig;
use crate::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};

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

/// Resolved local daemon instance id string (config override or hostname default).
pub fn local_instance_id_for_config(config: &DaemonConfig) -> String {
    config
        .daemon_instance_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::multi_host::local_daemon_instance_id().0)
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
    tokio::spawn(async move {
        loop {
            if let Err(e) =
                common_room_discovery_cycle(config.clone(), registry.clone(), room_slot.clone())
                    .await
            {
                log::warn!(
                    "common_room_discovery_cycle ended: {e:#} — clearing room handle and retrying"
                );
            }
            {
                let mut g = room_slot.write().await;
                *g = None;
            }
            registry.clear();
            tokio::time::sleep(Duration::from_secs(2)).await;
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

/// Connects to the common room and publishes daemon advertisement metadata.
async fn connect_common_room_publish_metadata(
    url: &str,
    token: &str,
    local_id: &str,
) -> anyhow::Result<(Arc<Room>, tokio::sync::mpsc::UnboundedReceiver<RoomEvent>)> {
    let (room, events) = Room::connect(url, token, RoomOptions::default()).await?;
    let room = Arc::new(room);
    let adv = DaemonAdvertisement {
        instance_id: local_id.to_string(),
        label: format!("{local_id} (this daemon)"),
    };
    let meta_json = serde_json::to_string(&adv)?;
    room.local_participant().set_metadata(meta_json).await?;
    Ok((room, events))
}

/// Runs the periodic + event-driven registry sync until the room disconnects or the event channel ends.
async fn run_common_room_registry_loop(
    room: Arc<Room>,
    mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    registry: Arc<CommonRoomPeerRegistry>,
    local_id: String,
) {
    registry.sync_from_room(room.as_ref(), &local_id);
    // 500 ms: safety net if participant events are delayed or missed; see module docs.
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                registry.sync_from_room(room.as_ref(), &local_id);
            }
            ev = events.recv() => {
                let Some(ev) = ev else {
                    log::info!("common_room_discovery: event channel closed");
                    break;
                };
                match ev {
                    RoomEvent::ParticipantConnected(p) => {
                        log::debug!(
                            "common_room_discovery: ParticipantConnected {:?}",
                            p.identity()
                        );
                        registry.sync_from_room(room.as_ref(), &local_id);
                    }
                    RoomEvent::ParticipantDisconnected(p) => {
                        log::debug!(
                            "common_room_discovery: ParticipantDisconnected {:?}",
                            p.identity()
                        );
                        registry.sync_from_room(room.as_ref(), &local_id);
                    }
                    RoomEvent::Disconnected { reason } => {
                        log::info!("common_room_discovery: room Disconnected {:?}", reason);
                        break;
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
) -> anyhow::Result<()> {
    let (room_name, url, api_key, api_secret) = livekit_common_room_connect_strings(&config)?;
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
    let (room, events) = connect_common_room_publish_metadata(&url, &token, &local_id).await?;
    {
        let mut g = room_slot.write().await;
        *g = Some(room.clone());
    }
    log::info!(
        "common_room_discovery: published daemon advertisement for instance_id={}",
        local_id
    );

    run_common_room_registry_loop(room, events, registry, local_id).await;
    Ok(())
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
}
