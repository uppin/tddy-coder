//! LiveKit `common_room` peer discovery: participant metadata advertisements, eligible daemon listing,
//! and **StartSession** routing via LiveKit data-channel RPC to peer daemons.
//!
//! # Advertisement JSON
//!
//! Published with [`livekit::prelude::LocalParticipant::set_metadata`]:
//! `{"instance_id":"<stable id>","label":"<human-readable>"}`.
//! Only participants that publish a **valid advertisement** are listed as eligible daemons; browser
//! (`web-`/`browser-`) and coder/session (`server…`, `daemon-<uuid>`) identities are excluded even
//! when their metadata looks advertisement-shaped (mirrors the web UI's `inferParticipantRole`).
//! There is no identity fallback — a participant with missing/invalid advertisement metadata is not
//! treated as a daemon.
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
use std::sync::Arc;
use std::time::{Duration, Instant};

use livekit::prelude::{
    ConnectionState, RemoteParticipant, Room, RoomError, RoomEvent, RoomOptions,
};
use livekit::DisconnectReason;
use prost::Message;
use serde::Deserialize;
use tddy_service::proto::connection::{
    AddProjectToHostRequest, AddProjectToHostResponse, DeleteSessionRequest, DeleteSessionResponse,
    ExecuteToolChunk, ExecuteToolRequest, ListProjectsRequest, ListProjectsResponse,
    ProjectEntry as ProtoProjectEntry, SetProjectDefaultBranchRequest,
    SetProjectDefaultBranchResponse, StartSessionEvent, StartSessionRequest, StartSessionResponse,
};
use tddy_service::proto::session_files::{
    DeleteStagedAttachmentRequest, DeleteStagedAttachmentResponse, HostDocumentChunk,
    ListStagedAttachmentsRequest, ListStagedAttachmentsResponse, ReadHostDocumentRequest,
    ReadHostDocumentResponse, UploadStagedAttachmentChunkRequest,
    UploadStagedAttachmentChunkResponse,
};

use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig};
use tddy_host_service::host_registry::{now_unix_ms, HostRegistry, HostSighting};
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};

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
/// Construct this in `tddy-daemon`'s `runtime::build` when `livekit.common_room` and credentials are
/// set; pass [`None`] to `tddy-daemon`'s `connection_service::ConnectionServiceImpl::new` for
/// single-host / discovery-disabled mode.
pub struct LiveKitDiscoveryHandles {
    pub eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    pub common_room_livekit_room: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
}

/// Payload published by a daemon so peers can show it in **ListEligibleDaemons**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DaemonAdvertisement {
    pub instance_id: String,
    pub label: String,
    /// The daemon's base clone location (`repos_base_path`, home-relative, e.g. `repos`), surfaced
    /// so the web Projects screen can show where a project would be cloned on this host. Empty when
    /// the daemon does not advertise one (older daemons / legacy advertisements).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repos_base_path: String,
    /// Largest single session attachment this host will serve (`max_attachment_bytes`), surfaced so
    /// the web Start-Session form can refuse an oversized document at pick time instead of after an
    /// upload. Zero when the daemon does not advertise one (older daemons / legacy advertisements),
    /// in which case the key is left off the wire so a reader cannot mistake it for a cap of zero.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max_attachment_bytes: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// A peer daemon as the common room describes it: what it advertises, plus the identity that
/// survives its restarts.
///
/// `host_id` is a **separate wire key** alongside the advertisement's own, rather than a field of
/// [`DaemonAdvertisement`], because that type is the payload the web already parses and its shape
/// is a published format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerDaemon {
    pub advertisement: DaemonAdvertisement,
    /// The daemon's identity **across restarts** — its instance id without the startup-timestamp
    /// suffix. Published so a peer can file this machine under one durable row however many times
    /// it reconnects with a fresh `instance_id`. A daemon that does not publish one (an older
    /// build) falls back to its `instance_id`, which is what it has always been filed under.
    pub host_id: String,
}

/// The JSON a daemon publishes as its common-room metadata.
///
/// `host_id` rides alongside the advertisement's keys so a reader that knows nothing about it —
/// the web, or an older daemon — sees exactly the advertisement it always saw.
#[derive(serde::Serialize)]
struct PublishedDaemonMetadata<'a> {
    #[serde(flatten)]
    advertisement: &'a DaemonAdvertisement,
    #[serde(skip_serializing_if = "str::is_empty")]
    host_id: &'a str,
}

/// The common-room metadata JSON for a daemon: its advertisement, plus its durable host id.
pub fn daemon_metadata_json(
    advertisement: &DaemonAdvertisement,
    host_id: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&PublishedDaemonMetadata {
        advertisement,
        host_id,
    })
}

#[derive(Debug, Deserialize)]
struct DaemonAdvertisementWire {
    instance_id: String,
    #[serde(default)]
    host_id: String,
    label: String,
    #[serde(default)]
    repos_base_path: String,
    #[serde(default)]
    max_attachment_bytes: u64,
}

/// Parse and normalize a daemon advertisement JSON string from the discovery transport.
pub fn parse_daemon_advertisement_json(input: &str) -> Result<DaemonAdvertisement, String> {
    parse_peer_daemon_json(input).map(|peer| peer.advertisement)
}

/// Parse a peer's common-room metadata: the advertisement, and the identity it keeps across its
/// restarts.
pub fn parse_peer_daemon_json(input: &str) -> Result<PeerDaemon, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty advertisement JSON".to_string());
    }
    let w: DaemonAdvertisementWire = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    let instance_id = w.instance_id.trim().to_string();
    let label = w.label.trim().to_string();
    let repos_base_path = w.repos_base_path.trim().to_string();
    if instance_id.is_empty() {
        return Err("advertisement instance_id is empty".to_string());
    }
    if label.is_empty() {
        return Err("advertisement label is empty".to_string());
    }
    // Metadata without a durable id predates the key; the instance id is what such a peer has
    // always been filed under, so falling back to it changes nothing for it.
    let host_id = match w.host_id.trim() {
        "" => instance_id.clone(),
        declared => declared.to_string(),
    };
    Ok(PeerDaemon {
        advertisement: DaemonAdvertisement {
            instance_id,
            label,
            repos_base_path,
            max_attachment_bytes: w.max_attachment_bytes,
        },
        host_id,
    })
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

/// Peer routing and peer forwarding, derived in [`tddy_daemon_kernel::peer_forwarding`].
///
/// They moved to the kernel because the subsystems that *route* — hosts first — are leaving this
/// crate, while the common-room connection they route over stays in it. Re-exported so every
/// caller's path is unchanged and there stays one implementation of each.
pub use tddy_daemon_kernel::peer_forwarding::{
    classify_peer_route, daemon_rpc_identity, forward_server_stream_to_peer, forward_to_peer,
    forward_to_peer_within, PeerRoute, PEER_FORWARD_STREAM_IDLE_TIMEOUT, PEER_FORWARD_TIMEOUT,
};

/// Backwards-compatible alias kept so that the internal `StartSession` handler (which references
/// the old name) continues to compile without a separate refactor pass.
pub type StartSessionPeerRoute = PeerRoute;

/// Backwards-compatible wrapper around [`classify_peer_route`].
pub fn classify_start_session_peer_route(
    local_instance_id: &str,
    requested_instance_id: &str,
    eligible_instance_ids: &[String],
) -> Result<StartSessionPeerRoute, String> {
    classify_peer_route(
        local_instance_id,
        requested_instance_id,
        eligible_instance_ids,
    )
}

/// Registry of remote daemons observed in the shared common room (excludes the local row).
///
/// Keyed by the peer's routing instance id, because that is what an RPC is forwarded to. Each row
/// keeps the whole advertisement, not just the eligible fields, so the durable host id and the host
/// facts a peer publishes are available to the registry sightings without a second parse.
#[derive(Default)]
pub struct CommonRoomPeerRegistry {
    remotes: std::sync::RwLock<HashMap<String, PeerDaemon>>,
    /// Where each snapshot's arrivals and departures are recorded durably.
    ///
    /// Optional because this registry is also the roster on its own: the discovery tests and any
    /// caller that only cares about live membership build one without persistence.
    host_registry: Option<Arc<dyn HostRegistry>>,
}

impl std::fmt::Debug for CommonRoomPeerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A poisoned lock is reported as such: printing `0` there would make "a writer panicked
        // holding this" indistinguishable from "nobody is in the room", in the one output someone
        // reads while working out which of those happened.
        let remotes: Box<dyn std::fmt::Debug> = match self.remotes.read() {
            Ok(g) => Box::new(g.len()),
            Err(_) => Box::new("<poisoned>"),
        };
        f.debug_struct("CommonRoomPeerRegistry")
            .field("remotes", &remotes)
            .field("persists", &self.host_registry.is_some())
            .finish()
    }
}

impl CommonRoomPeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record every snapshot transition into `host_registry` as well as the live map.
    #[must_use]
    pub fn with_host_registry(mut self, host_registry: Arc<dyn HostRegistry>) -> Self {
        self.host_registry = Some(host_registry);
        self
    }

    /// Replace remote entries from a full room snapshot (authoritative for membership).
    pub fn sync_from_room(&self, room: &Room, local_instance_id: &str) {
        let mut next: HashMap<String, PeerDaemon> = HashMap::new();
        for (_, participant) in room.remote_participants() {
            if let Some(peer) = remote_participant_to_peer(participant, local_instance_id) {
                log::debug!(
                    target: LOG_LIVEKIT_PEER_METADATA,
                    "CommonRoomPeerRegistry: sync sees remote instance_id={} host_id={} label_len={}",
                    peer.advertisement.instance_id,
                    peer.host_id,
                    peer.advertisement.label.len()
                );
                next.insert(peer.advertisement.instance_id.clone(), peer);
            }
        }
        self.apply_snapshot(next);
    }

    /// Adopt `next` as the room's membership and record the transition.
    ///
    /// Split from [`Self::sync_from_room`] so the recording behaviour can be exercised without a
    /// live LiveKit `Room`: everything below this line is about the difference between two
    /// snapshots, and nothing about it is LiveKit's.
    fn apply_snapshot(&self, next: HashMap<String, PeerDaemon>) {
        let n = next.len();
        let sightings: Vec<HostSighting> = next.values().map(host_sighting_from_peer).collect();
        // Taking the old map out as the new one goes in is what makes a departure observable at
        // all: the snapshot is authoritative about who is present and says nothing about who left,
        // so the difference against what was here a moment ago is the only evidence there is.
        let previous = {
            let mut g = self.remotes.write().expect("registry lock");
            std::mem::replace(&mut *g, next)
        };
        // Deliberately outside the guard above: recording writes a file, and the routing map must
        // not be locked while that happens.
        self.record_snapshot(&sightings, previous.into_values());
        log::info!(
            "CommonRoomPeerRegistry: synced {} remote daemon(s) from LiveKit room snapshot",
            n
        );
    }

    /// Persist this snapshot: every peer visible now was seen, and every host in `previous` that is
    /// no longer visible has departed.
    ///
    /// One call, not one per peer: the tick that drives this runs every 500 ms, and a registry
    /// write is a whole-file republish with two fsyncs, so a peer-at-a-time loop would turn a
    /// three-machine room into a dozen fsyncs a second on a runtime worker that also serves RPCs.
    /// The store then decides whether this snapshot changed anything at all, and an unchanged one
    /// writes nothing.
    ///
    /// The write stays on this runtime worker rather than moving to `spawn_blocking`. It is only
    /// reached on a real transition now — an arrival, a departure or a changed column — so it is
    /// rare rather than periodic, and handing snapshots to a task pool would either need a queue
    /// (unbounded spawning under a flapping peer) or lose their ordering, which is the one thing a
    /// sequence of snapshots cannot afford.
    ///
    /// A failed write is logged and dropped. Discovery's job is routing, and a persistence failure
    /// costs a stale row on a screen — whereas propagating it would let a full disk take the peer
    /// roster, and every RPC routed through it, down with it.
    fn record_snapshot(
        &self,
        sightings: &[HostSighting],
        previous: impl Iterator<Item = PeerDaemon>,
    ) {
        let Some(host_registry) = self.host_registry.as_ref() else {
            return;
        };
        let departed: Vec<DaemonInstanceId> = previous
            .map(|peer| peer.host_id)
            .filter(|host_id| !sightings.iter().any(|seen| &seen.instance_id.0 == host_id))
            .map(DaemonInstanceId)
            .collect();
        if let Err(e) = host_registry.record_snapshot(sightings, &departed, now_unix_ms()) {
            log::warn!("host registry: could not record the room snapshot: {e}");
        }
    }

    pub fn snapshot_remotes(&self) -> Vec<EligibleDaemonInfo> {
        self.remotes
            .read()
            .expect("registry lock")
            .values()
            .cloned()
            .map(eligible_daemon_from_peer)
            .collect()
    }

    /// The remote peers as the host registry knows them: durable host id and label.
    fn snapshot_live_known_hosts(&self) -> Vec<EligibleDaemonInfo> {
        self.remotes
            .read()
            .expect("registry lock")
            .values()
            .map(|peer| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(peer.host_id.clone()),
                label: peer.advertisement.label.clone(),
            })
            .collect()
    }

    /// Drop all remote rows (e.g. when discovery disconnects).
    pub fn clear(&self) {
        let mut g = self.remotes.write().expect("registry lock");
        g.clear();
        log::debug!("CommonRoomPeerRegistry: cleared all remote daemon entries");
    }
}

/// The eligible row a validated peer stands for.
fn eligible_daemon_from_peer(peer: PeerDaemon) -> EligibleDaemonInfo {
    EligibleDaemonInfo {
        instance_id: DaemonInstanceId(peer.advertisement.instance_id),
        label: peer.advertisement.label,
    }
}

/// The durable sighting a validated peer stands for.
///
/// Keyed by the peer's durable host id, and carrying the host facts the advertisement already
/// publishes — without this the Hosts screen's repos-base-path column would be empty for every
/// machine except the one serving the page.
fn host_sighting_from_peer(peer: &PeerDaemon) -> HostSighting {
    HostSighting {
        instance_id: DaemonInstanceId(peer.host_id.clone()),
        label: peer.advertisement.label.clone(),
        repos_base_path: Some(peer.advertisement.repos_base_path.clone()).filter(|p| !p.is_empty()),
        // Zero is how an advertisement spells "not advertised" (the key is left off the wire), so
        // it must not be recorded as a cap of zero.
        max_attachment_bytes: Some(peer.advertisement.max_attachment_bytes).filter(|b| *b != 0),
    }
}

/// Classify a common-room participant, returning its daemon advertisement **only** when the
/// participant is a genuine `tddy-daemon` — not a browser or a coder/session participant.
///
/// Mirrors the web UI's `inferParticipantRole` (`tddy-web/src/hooks/useRoomParticipants.ts`):
/// browser identities (`web-`/`browser-`) and coder/session identities (`server`, `server…`,
/// `daemon-<uuid>…`) are never daemons — even when they publish advertisement-shaped metadata — and
/// a daemon must publish a valid advertisement (no identity fallback). Only daemons own projects, so
/// only daemons are eligible hosts for session/project routing and project fan-out.
///
/// The whole advertisement comes back rather than the eligible row alone, because the durable host
/// registry needs the columns the row drops: the peer's restart-surviving `host_id`, its repos base
/// path and its attachment cap.
fn peer_daemon_from_participant_fields(
    identity: &str,
    metadata: &str,
    local_instance_id: &str,
) -> Option<PeerDaemon> {
    let id_trim = identity.trim();
    if id_trim.starts_with("web-") || id_trim.starts_with("browser-") {
        return None;
    }
    // A coder/session participant joins with a `server…` or `daemon-<uuid>` identity; it is never a
    // host daemon even if its metadata happens to look like an advertisement.
    if id_trim == "server" || id_trim.starts_with("server") || id_trim.starts_with("daemon-") {
        return None;
    }
    // A split session's agent holds a join token granting `can_update_own_metadata`, and this
    // function's only evidence is self-declared metadata — so an agent running model-authored code
    // could otherwise publish a daemon advertisement and insert a host of its choosing into every
    // daemon's eligible list and the web's host picker. Its identity prefix is reserved for exactly
    // this refusal (`tddy_daemon_kernel::daemon_identity::SPLIT_AGENT_IDENTITY_PREFIX`).
    if id_trim.starts_with(tddy_daemon_kernel::daemon_identity::SPLIT_AGENT_IDENTITY_PREFIX) {
        return None;
    }
    let peer = parse_peer_daemon_json(metadata.trim()).ok()?;
    let instance_id = peer.advertisement.instance_id.trim().to_string();
    if instance_id.is_empty() || instance_id == local_instance_id.trim() {
        return None;
    }
    let label = if peer.advertisement.label.trim().is_empty() {
        format!("{instance_id} (LiveKit peer)")
    } else {
        peer.advertisement.label.trim().to_string()
    };
    Some(PeerDaemon {
        advertisement: DaemonAdvertisement {
            instance_id,
            label,
            ..peer.advertisement
        },
        ..peer
    })
}

fn remote_participant_to_peer(
    remote: RemoteParticipant,
    local_instance_id: &str,
) -> Option<PeerDaemon> {
    let identity_str = remote.identity().to_string();
    let meta = remote.metadata();
    let result = peer_daemon_from_participant_fields(&identity_str, &meta, local_instance_id);
    if result.is_none() {
        log::debug!(
            target: LOG_LIVEKIT_PEER_METADATA,
            "peer {} is not an eligible daemon (browser/coder identity or no valid daemon advertisement); skipping",
            identity_str
        );
    }
    result
}

/// This daemon's own two ids.
///
/// They are derived in [`tddy_daemon_kernel::daemon_identity`] rather than here because the spawn
/// layer and the host subsystem need them too, and those now live in other crates. Re-exported so
/// every caller's path is unchanged, and so there stays exactly one derivation of each.
pub use tddy_daemon_kernel::daemon_identity::{
    local_base_instance_id_for_config, local_instance_id_for_config,
};

/// LiveKit-backed **EligibleDaemonSource** — reads [`CommonRoomPeerRegistry`] populated by the discovery task.
pub struct LiveKitEligibleDaemonSource {
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
}

impl LiveKitEligibleDaemonSource {
    pub fn new(
        config: Arc<DaemonConfig>,
        registry: Arc<CommonRoomPeerRegistry>,
        room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    ) -> Self {
        Self {
            config,
            registry,
            room_slot,
        }
    }

    fn local_row(&self) -> EligibleDaemonInfo {
        tddy_host_service::multi_host::eligible_daemon_entry_for(DaemonInstanceId(
            local_instance_id_for_config(&self.config),
        ))
    }

    /// The local row under the id that survives a restart, for the host registry's join.
    fn local_durable_row(&self) -> EligibleDaemonInfo {
        tddy_host_service::multi_host::eligible_daemon_entry_for(DaemonInstanceId(
            local_base_instance_id_for_config(&self.config),
        ))
    }
}

#[async_trait::async_trait]
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

    fn live_known_hosts(&self) -> Vec<EligibleDaemonInfo> {
        let local = self.local_durable_row();
        // Same rows, keyed by the id that survives a restart — each peer's own advertised
        // `host_id`, and this daemon's un-suffixed instance id.
        merge_discovered_peers_ordered(local, self.registry.snapshot_live_known_hosts())
            .unwrap_or_else(|e| {
                log::warn!(
                    "merge_discovered_peers_ordered failed for the durable roster: {} — returning local only",
                    e
                );
                vec![self.local_durable_row()]
            })
    }

    /// Fan out to each discovered peer's `ListProjects` (with `local_only = true` to avoid recursive
    /// fan-out) and tag every returned row with that peer's instance id. Unreachable peers are
    /// logged and skipped so aggregation degrades to whatever rows could be gathered.
    async fn peer_project_entries(&self, session_token: &str) -> Vec<ProtoProjectEntry> {
        let remotes = self.registry.snapshot_remotes();
        if remotes.is_empty() {
            return Vec::new();
        }
        let room_slot = self.room_slot.clone();
        let session_token = session_token.to_string();
        let peer_ids: Vec<String> = remotes.into_iter().map(|r| r.instance_id.0).collect();
        aggregate_peer_project_entries(peer_ids, PEER_PROJECT_FANOUT_TIMEOUT, move |peer_id| {
            let room_slot = room_slot.clone();
            let session_token = session_token.clone();
            async move {
                let req = ListProjectsRequest {
                    session_token,
                    local_only: true,
                };
                let bytes = forward_to_peer(
                    &room_slot,
                    &peer_id,
                    "connection.ConnectionService",
                    "ListProjects",
                    req.encode_to_vec(),
                )
                .await?;
                ListProjectsResponse::decode(bytes.as_slice())
                    .map(|resp| resp.projects)
                    .map_err(|e| {
                        tddy_rpc::Status::internal(format!(
                            "decode ListProjectsResponse from peer {peer_id}: {e}"
                        ))
                    })
            }
        })
        .await
    }
}

/// Per-peer timeout for the [`aggregate_peer_project_entries`] fan-out. A peer whose RPC endpoint
/// has gone (its unprefixed discovery identity can still linger in the common room after
/// `daemon-<id>` times out) never answers a forwarded `ListProjects`; without this bound one such
/// peer blocks the whole aggregated response and the project list never loads on any daemon.
pub const PEER_PROJECT_FANOUT_TIMEOUT: Duration = Duration::from_secs(3);

/// Aggregate project rows across `peer_ids`, bounding each peer's `forward` by `per_peer_timeout`.
/// Rows from a responsive peer are tagged with that peer's instance id; a peer that times out or
/// errors is logged and skipped, so aggregation returns whatever the responsive peers provided
/// rather than blocking on a dead one. `forward` yields a single peer's project rows.
pub async fn aggregate_peer_project_entries<F, Fut>(
    peer_ids: Vec<String>,
    per_peer_timeout: Duration,
    forward: F,
) -> Vec<ProtoProjectEntry>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<ProtoProjectEntry>, tddy_rpc::Status>>,
{
    // Fan out to every peer concurrently so the aggregate is bounded by the slowest responsive
    // peer (or `per_peer_timeout`), not the serial sum across peers. Each peer's rows are tagged
    // with its instance id; a peer that errors or times out contributes nothing.
    let per_peer = peer_ids.into_iter().map(|peer_id| {
        let fut = forward(peer_id.clone());
        async move {
            match tokio::time::timeout(per_peer_timeout, fut).await {
                Ok(Ok(rows)) => rows
                    .into_iter()
                    .map(|mut p| {
                        p.daemon_instance_id = peer_id.clone();
                        p
                    })
                    .collect(),
                Ok(Err(e)) => {
                    log::warn!(
                        "aggregate_peer_project_entries: forward ListProjects to peer {} failed: {}",
                        peer_id,
                        e
                    );
                    Vec::new()
                }
                Err(_elapsed) => {
                    log::warn!(
                        "aggregate_peer_project_entries: peer {} did not respond within {:?}; skipping",
                        peer_id,
                        per_peer_timeout
                    );
                    Vec::new()
                }
            }
        }
    });
    futures_util::future::join_all(per_peer)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// The discovery loop alone: join the common room, publish this daemon's advertisement, keep
/// `registry` and `room_slot` in sync, and reconnect for ever when the room drops it.
///
/// Returned as a handle because it never ends on its own: the only way to stop discovering peers in
/// a room is for its owner to abort it, which is what a reconfigured common room does.
pub fn spawn_common_room_discovery_loop(
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
) -> tokio::task::JoinHandle<()> {
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
    })
}

/// Validates `livekit` + `common_room` URL/key/secret strings for discovery, returning
/// `(room_name, url, api_key, api_secret)`.
///
/// Also the source of truth for minting a **scoped** join token for a process this daemon spawns
/// into the same room (a split session's agent), so the room it is granted is exactly the room this
/// daemon's peer routing rides.
///
/// `pub` rather than `pub(crate)` because the callers that spawn such a process — `split_session`
/// and the hosted agent clone — stayed in `tddy-daemon` when this module left. Widened as a
/// consequence of the crate boundary, not as a new entry point: this stays the one place the
/// common room's connect strings are validated.
pub fn livekit_common_room_connect_strings(
    config: &DaemonConfig,
) -> anyhow::Result<(String, String, String, String)> {
    let livekit = config
        .livekit
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("LiveKit not configured"))?;
    // The operator's switch, asked before any field is: a daemon told not to join and a daemon
    // that cannot are different operator problems, and must read differently.
    anyhow::ensure!(
        LiveKitConfig::common_room_enabled(Some(livekit)),
        "the common room is disabled (livekit.enabled is false)"
    );
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
    host_id: &str,
    repos_base_path: &str,
    max_attachment_bytes: u64,
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
        repos_base_path: repos_base_path.to_string(),
        max_attachment_bytes,
    };
    let meta_json = daemon_metadata_json(&adv, host_id)?;
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
#[allow(clippy::too_many_arguments)] // room/event/registry wiring; a struct would obscure call sites
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
    let repos_base_path = config.repos_base_path_or_default().to_string();
    let host_id = local_base_instance_id_for_config(&config);
    let (room, events, event_buffer, daemon_adv_metadata) = connect_common_room_publish_metadata(
        &room_name,
        &url,
        &token,
        &local_id,
        &host_id,
        &repos_base_path,
        config.max_attachment_bytes,
    )
    .await?;
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
/// Thin encode/decode wrapper around [`forward_to_peer`].
pub async fn forward_start_session_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<StartSessionResponse, tddy_rpc::Status> {
    forward_start_session_via_livekit_within(
        room_slot,
        peer_instance_id,
        request,
        PEER_FORWARD_TIMEOUT,
    )
    .await
}

/// [`forward_start_session_via_livekit`] with an explicit deadline.
///
/// A start the peer serves by cloning a project and cutting a worktree outlasts the ordinary forward
/// deadline; see [`forward_to_peer_within`].
pub async fn forward_start_session_via_livekit_within(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &StartSessionRequest,
    deadline: Duration,
) -> Result<StartSessionResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer_within(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "StartSession",
        body,
        deadline,
    )
    .await?;
    StartSessionResponse::decode(out.as_slice())
        .map_err(|e| tddy_rpc::Status::internal(format!("decode StartSessionResponse: {e}")))
}

/// Forward **StreamStartSession** to another daemon in the common room, yielding the peer's
/// start-session events (attachment progress, then the terminal result).
///
/// Thin decode wrapper around [`forward_server_stream_to_peer`]. The session — and its
/// attachments — live on the peer; only the events cross back.
pub async fn forward_stream_start_session_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &StartSessionRequest,
) -> Result<
    tokio::sync::mpsc::UnboundedReceiver<Result<StartSessionEvent, tddy_rpc::Status>>,
    tddy_rpc::Status,
> {
    forward_server_stream_to_peer(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "StreamStartSession",
        request.encode_to_vec(),
        |bytes| {
            StartSessionEvent::decode(bytes.as_slice()).map_err(|e| {
                tddy_rpc::Status::internal(format!("decode StartSessionEvent from peer: {e}"))
            })
        },
    )
    .await
}

/// Forward **DeleteSession** to another daemon in the common room via LiveKit data-channel RPC.
///
/// `DeleteSessionRequest` carries no `daemon_instance_id`, so the peer is addressed explicitly. Used
/// to delete the paired `workspace` session that holds a split session's worktree — including the
/// teardown of a split start that failed after the peer had already created it.
pub async fn forward_delete_session_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &DeleteSessionRequest,
) -> Result<DeleteSessionResponse, tddy_rpc::Status> {
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "DeleteSession",
        request.encode_to_vec(),
    )
    .await?;
    DeleteSessionResponse::decode(out.as_slice())
        .map_err(|e| tddy_rpc::Status::internal(format!("decode DeleteSessionResponse: {e}")))
}

/// Forward **StreamExecuteTool** to another daemon in the common room, yielding the peer's result
/// frames.
///
/// Thin decode wrapper around [`forward_server_stream_to_peer`]: a frame that fails to decode — or a
/// stream that stops without its `last` frame — terminates the relay with a status, so a caller
/// reassembling a tool result never mistakes a truncated one for the whole answer.
pub async fn forward_stream_execute_tool_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &ExecuteToolRequest,
) -> Result<
    tokio::sync::mpsc::UnboundedReceiver<Result<ExecuteToolChunk, tddy_rpc::Status>>,
    tddy_rpc::Status,
> {
    forward_server_stream_to_peer(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "StreamExecuteTool",
        request.encode_to_vec(),
        |bytes| {
            ExecuteToolChunk::decode(bytes.as_slice()).map_err(|e| {
                tddy_rpc::Status::internal(format!("decode ExecuteToolChunk from peer: {e}"))
            })
        },
    )
    .await
}

/// Forward **AddProjectToHost** to another daemon in the common room via LiveKit data-channel RPC.
///
/// Thin encode/decode wrapper around [`forward_to_peer`].
pub async fn forward_add_project_to_host_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &AddProjectToHostRequest,
) -> Result<AddProjectToHostResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "AddProjectToHost",
        body,
    )
    .await?;
    AddProjectToHostResponse::decode(out.as_slice())
        .map_err(|e| tddy_rpc::Status::internal(format!("decode AddProjectToHostResponse: {e}")))
}

/// Forward **SetProjectDefaultBranch** to another daemon in the common room via LiveKit data-channel
/// RPC.
///
/// Thin encode/decode wrapper around [`forward_to_peer`].
pub async fn forward_set_project_default_branch_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &SetProjectDefaultBranchRequest,
) -> Result<SetProjectDefaultBranchResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        "connection.ConnectionService",
        "SetProjectDefaultBranch",
        body,
    )
    .await?;
    SetProjectDefaultBranchResponse::decode(out.as_slice()).map_err(|e| {
        tddy_rpc::Status::internal(format!("decode SetProjectDefaultBranchResponse: {e}"))
    })
}

/// Forward **UploadStagedAttachmentChunk** to another daemon in the common room via LiveKit
/// data-channel RPC.
///
/// This and the four session-file forwarders below address
/// [`tddy_service::SESSION_FILES_SERVICE`] rather than spelling the coordinate out: the peer serves
/// it under that same constant, and a forward addressed at a name the peer does not serve fails on
/// the peer at runtime — see the constant's own documentation.
pub async fn forward_upload_staged_attachment_chunk_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &UploadStagedAttachmentChunkRequest,
) -> Result<UploadStagedAttachmentChunkResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "UploadStagedAttachmentChunk",
        body,
    )
    .await?;
    UploadStagedAttachmentChunkResponse::decode(out.as_slice()).map_err(|e| {
        tddy_rpc::Status::internal(format!("decode UploadStagedAttachmentChunkResponse: {e}"))
    })
}

/// Forward **ListStagedAttachments** to another daemon in the common room via LiveKit
/// data-channel RPC.
pub async fn forward_list_staged_attachments_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &ListStagedAttachmentsRequest,
) -> Result<ListStagedAttachmentsResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "ListStagedAttachments",
        body,
    )
    .await?;
    ListStagedAttachmentsResponse::decode(out.as_slice()).map_err(|e| {
        tddy_rpc::Status::internal(format!("decode ListStagedAttachmentsResponse: {e}"))
    })
}

/// Forward **DeleteStagedAttachment** to another daemon in the common room via LiveKit
/// data-channel RPC.
pub async fn forward_delete_staged_attachment_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &DeleteStagedAttachmentRequest,
) -> Result<DeleteStagedAttachmentResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "DeleteStagedAttachment",
        body,
    )
    .await?;
    DeleteStagedAttachmentResponse::decode(out.as_slice()).map_err(|e| {
        tddy_rpc::Status::internal(format!("decode DeleteStagedAttachmentResponse: {e}"))
    })
}

/// Forward **ReadHostDocument** to another daemon in the common room via LiveKit data-channel RPC.
pub async fn forward_read_host_document_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<ReadHostDocumentResponse, tddy_rpc::Status> {
    let body = request.encode_to_vec();
    let out = forward_to_peer(
        room_slot,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "ReadHostDocument",
        body,
    )
    .await?;
    ReadHostDocumentResponse::decode(out.as_slice())
        .map_err(|e| tddy_rpc::Status::internal(format!("decode ReadHostDocumentResponse: {e}")))
}

/// Forward **StreamReadHostDocument** to another daemon in the common room, yielding the peer's
/// document frames.
///
/// Thin decode wrapper around [`forward_server_stream_to_peer`]: a frame that fails to decode
/// terminates the stream with a status, so a caller reassembling the document never treats a
/// malformed frame as the document's end.
pub async fn forward_stream_read_host_document_via_livekit(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_instance_id: &str,
    request: &ReadHostDocumentRequest,
) -> Result<
    tokio::sync::mpsc::UnboundedReceiver<Result<HostDocumentChunk, tddy_rpc::Status>>,
    tddy_rpc::Status,
> {
    forward_server_stream_to_peer(
        room_slot,
        peer_instance_id,
        tddy_service::SESSION_FILES_SERVICE,
        "StreamReadHostDocument",
        request.encode_to_vec(),
        |bytes| {
            HostDocumentChunk::decode(bytes.as_slice()).map_err(|e| {
                tddy_rpc::Status::internal(format!("decode HostDocumentChunk from peer: {e}"))
            })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tddy_host_service::multi_host::DaemonInstanceId;

    // -----------------------------------------------------------------------
    // Recording the room into the durable host registry.
    //
    // A snapshot is the unit: the tick fires every 500 ms, and the registry's writes are
    // whole-file republishes, so the discovery path must hand over one snapshot per observation
    // rather than one call per peer.
    // -----------------------------------------------------------------------

    /// What the registry was told, in order.
    #[derive(Default)]
    struct RecordingHostRegistry {
        snapshots: std::sync::Mutex<Vec<(Vec<HostSighting>, Vec<DaemonInstanceId>)>>,
    }

    impl RecordingHostRegistry {
        fn snapshots(&self) -> Vec<(Vec<HostSighting>, Vec<DaemonInstanceId>)> {
            self.snapshots.lock().expect("recorded snapshots").clone()
        }
    }

    impl HostRegistry for RecordingHostRegistry {
        fn record_snapshot(
            &self,
            seen: &[HostSighting],
            departed: &[DaemonInstanceId],
            _now_unix_ms: i64,
        ) -> Result<(), String> {
            self.snapshots
                .lock()
                .expect("recorded snapshots")
                .push((seen.to_vec(), departed.to_vec()));
            Ok(())
        }

        fn known_hosts(
            &self,
            _live_roster: &[EligibleDaemonInfo],
            _local: &HostSighting,
            _now_unix_ms: i64,
        ) -> Vec<tddy_host_service::host_registry::KnownHostView> {
            Vec::new()
        }
    }

    /// A room holding the named peers, each advertising a durable host id of `<id>-host`.
    fn a_room_of(instance_ids: &[&str]) -> HashMap<String, PeerDaemon> {
        instance_ids
            .iter()
            .map(|id| {
                let peer = PeerDaemon {
                    advertisement: DaemonAdvertisement {
                        instance_id: (*id).to_string(),
                        label: format!("{id} (this daemon)"),
                        repos_base_path: format!("repos/{id}"),
                        max_attachment_bytes: 4096,
                    },
                    host_id: format!("{id}-host"),
                };
                (peer.advertisement.instance_id.clone(), peer)
            })
            .collect()
    }

    fn a_registry_recording_into(recorder: &Arc<RecordingHostRegistry>) -> CommonRoomPeerRegistry {
        CommonRoomPeerRegistry::new()
            .with_host_registry(Arc::clone(recorder) as Arc<dyn HostRegistry>)
    }

    fn seen_host_ids(seen: &[HostSighting]) -> Vec<String> {
        let mut ids: Vec<String> = seen.iter().map(|s| s.instance_id.0.clone()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_room_snapshot_is_recorded_once_for_every_peer_it_holds() {
        // Given a peer registry recording into the durable host registry
        let recorder = Arc::new(RecordingHostRegistry::default());
        let registry = a_registry_recording_into(&recorder);

        // When a room holding two peers is observed
        registry.apply_snapshot(a_room_of(&["peer-a", "peer-b"]));

        // Then both were recorded, in a single snapshot — a call per peer would be a whole-file
        // republish per peer, several times a second, forever
        let snapshots = recorder.snapshots();
        assert_eq!(snapshots.len(), 1, "one observation is one recording");
        assert_eq!(
            seen_host_ids(&snapshots[0].0),
            vec!["peer-a-host".to_string(), "peer-b-host".to_string()]
        );
        assert!(
            snapshots[0].1.is_empty(),
            "nobody left a room that was empty a moment ago"
        );
    }

    #[test]
    fn a_peer_that_leaves_the_room_is_recorded_as_departed_and_the_one_that_stays_is_not() {
        // Given a room that held two peers
        let recorder = Arc::new(RecordingHostRegistry::default());
        let registry = a_registry_recording_into(&recorder);
        registry.apply_snapshot(a_room_of(&["peer-a", "peer-b"]));

        // When only one of them is still there
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // Then the one still present is a sighting, and only the missing one is a departure
        let snapshots = recorder.snapshots();
        let (seen, departed) = &snapshots[1];
        assert_eq!(seen_host_ids(seen), vec!["peer-a-host".to_string()]);
        assert_eq!(
            departed,
            &vec![DaemonInstanceId("peer-b-host".to_string())],
            "the peer that stayed must not be recorded as gone"
        );
    }

    #[test]
    fn a_repeated_room_snapshot_records_no_departure() {
        // Given a room with one peer in it
        let recorder = Arc::new(RecordingHostRegistry::default());
        let registry = a_registry_recording_into(&recorder);
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // When the very same membership is observed again, as the 500 ms tick does
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // Then the tick invents nothing; the store decides that this changes nothing and writes
        // no file (`host_registry`: a_snapshot_that_changes_nothing_does_not_rewrite_the_file)
        let snapshots = recorder.snapshots();
        assert!(
            snapshots.iter().all(|(_, departed)| departed.is_empty()),
            "a peer that never left is never recorded as gone"
        );
    }

    #[test]
    fn a_peer_sighting_carries_the_host_facts_the_peer_advertises() {
        // Given a peer advertising where it clones and how large an attachment it will take
        let recorder = Arc::new(RecordingHostRegistry::default());
        let registry = a_registry_recording_into(&recorder);

        // When the room is observed
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // Then the sighting carries them, so the Hosts screen has a repos base path for a machine
        // other than the one serving the page
        let snapshots = recorder.snapshots();
        let sighting = &snapshots[0].0[0];
        assert_eq!(sighting.repos_base_path.as_deref(), Some("repos/peer-a"));
        assert_eq!(sighting.max_attachment_bytes, Some(4096));
    }

    #[test]
    fn a_peer_is_recorded_under_the_host_id_it_advertises_not_its_per_run_instance_id() {
        // Given a peer whose instance id is unique to this run of it
        let recorder = Arc::new(RecordingHostRegistry::default());
        let registry = a_registry_recording_into(&recorder);

        // When the room is observed
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // Then it is filed under the id that survives its restarts — otherwise "never delete a
        // host" would leave a permanent extra row for every restart of the same machine
        let snapshots = recorder.snapshots();
        assert_eq!(snapshots[0].0[0].instance_id.0, "peer-a-host");
    }

    #[test]
    fn a_peer_registry_with_no_host_registry_records_nothing_and_still_routes() {
        // Given a peer registry built without persistence, as the routing-only callers do
        let registry = CommonRoomPeerRegistry::new();

        // When a room is observed
        registry.apply_snapshot(a_room_of(&["peer-a"]));

        // Then the live roster is still there to route on
        assert_eq!(
            registry.snapshot_remotes()[0].instance_id,
            DaemonInstanceId("peer-a".to_string())
        );
    }

    #[test]
    fn parse_daemon_advertisement_accepts_documented_json_shape() {
        let json = r#"{"instance_id":"peer-a","label":"Peer A"}"#;
        let got =
            parse_daemon_advertisement_json(json).expect("parse documented advertisement JSON");
        assert_eq!(got.instance_id, "peer-a");
        assert_eq!(got.label, "Peer A");
        assert_eq!(
            got.repos_base_path, "",
            "an advertisement without repos_base_path parses to an empty base path"
        );
    }

    #[test]
    fn daemon_advertisement_round_trips_the_repos_base_path() {
        // Given a daemon advertising its base clone location
        let adv = DaemonAdvertisement {
            instance_id: "peer-a".to_string(),
            label: "peer-a (this daemon)".to_string(),
            repos_base_path: "repos".to_string(),
            max_attachment_bytes: 0,
        };

        // When it is serialized to the wire and parsed back
        let json = serde_json::to_string(&adv).expect("serialize advertisement");
        let got = parse_daemon_advertisement_json(&json).expect("parse advertisement JSON");

        // Then the base clone location survives the round trip under the `repos_base_path` key
        assert!(
            json.contains("\"repos_base_path\":\"repos\""),
            "advertisement JSON must carry the repos_base_path key: {json}"
        );
        assert_eq!(got.repos_base_path, "repos");
    }

    #[test]
    fn eligible_daemon_accepts_a_peer_daemon_advertisement() {
        // Given a genuine peer daemon (unprefixed identity + advertisement metadata)
        let meta = r#"{"instance_id":"udoo","label":"udoo (this daemon)"}"#;

        // When
        let got = peer_daemon_from_participant_fields("udoo", meta, "local-host");

        // Then
        let got = got.expect("a peer daemon advertisement is eligible");
        assert_eq!(got.advertisement.instance_id, "udoo");
        assert_eq!(got.advertisement.label, "udoo (this daemon)");
        assert_eq!(
            got.host_id, "udoo",
            "metadata from before the host_id key falls back to the instance id, which is what such a peer has always been filed under"
        );
    }

    #[test]
    fn eligible_daemon_rejects_a_coder_session_participant_even_with_advertisement_metadata() {
        // Given a coder/session participant (identity `daemon-<uuid>`) that publishes
        // advertisement-shaped metadata — only real daemons own projects, so it must be excluded.
        let meta = r#"{"instance_id":"proj-x","label":"proj-x (this daemon)"}"#;

        // When
        let got = peer_daemon_from_participant_fields(
            "daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0",
            meta,
            "local-host",
        );

        // Then
        assert!(
            got.is_none(),
            "a coder-role participant is not an eligible daemon"
        );
    }

    #[test]
    fn eligible_daemon_rejects_a_split_session_agent_participant() {
        // Given a split session's agent, whose scoped join token lets it publish its own metadata:
        // an agent running model-authored code could otherwise advertise itself as a daemon and put
        // an arbitrary host into every peer's eligible list and the web's host picker
        let meta = r#"{"instance_id":"attacker-host","label":"Build server"}"#;

        // When
        let got = peer_daemon_from_participant_fields(
            "split-agent-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0",
            meta,
            "local-host",
        );

        // Then
        assert!(
            got.is_none(),
            "a split session's agent is not an eligible daemon, whatever metadata it publishes"
        );
    }

    #[test]
    fn eligible_daemon_rejects_a_server_coder_identity() {
        // Given
        let got = peer_daemon_from_participant_fields("server", "", "local-host");

        // Then
        assert!(
            got.is_none(),
            "the coder `server` identity is not an eligible daemon"
        );
    }

    #[test]
    fn eligible_daemon_rejects_a_browser_participant() {
        // Given
        let got = peer_daemon_from_participant_fields("web-u-1-x", "", "local-host");

        // Then
        assert!(
            got.is_none(),
            "a browser participant is not an eligible daemon"
        );
    }

    #[test]
    fn eligible_daemon_rejects_a_participant_without_a_valid_advertisement() {
        // Given a peer with a plausible-but-unprefixed identity and no advertisement metadata:
        // there is no identity fallback, so it is not treated as a daemon.
        let got = peer_daemon_from_participant_fields("random-peer", "", "local-host");

        // Then
        assert!(
            got.is_none(),
            "a daemon must publish a valid advertisement to be eligible"
        );
    }

    #[test]
    fn eligible_daemon_excludes_the_local_instance() {
        // Given the local daemon's own advertisement
        let meta = r#"{"instance_id":"local-host","label":"local-host (this daemon)"}"#;

        // When
        let got = peer_daemon_from_participant_fields("local-host", meta, "local-host");

        // Then
        assert!(
            got.is_none(),
            "the local instance is not listed as a remote peer"
        );
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
        let room_slot = Arc::new(tokio::sync::RwLock::new(None));
        let src = LiveKitEligibleDaemonSource::new(config, registry, room_slot);
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

    // -----------------------------------------------------------------------
    // Peer project aggregation must degrade gracefully around an unreachable peer.
    //
    // A peer whose discovery identity lingers in the room after its RPC endpoint has gone (observed
    // live: `daemon-mac` disconnected on ConnectionTimeout while `mac` reconnected) accepts a
    // forwarded `ListProjects` that never gets answered. Without a per-peer bound the fan-out blocks
    // the whole `ListProjects` response, so no project list loads on any daemon. Aggregation must
    // bound each peer and skip the ones that time out or error, returning the responsive rows.
    // -----------------------------------------------------------------------

    fn a_project_entry(project_id: &str) -> ProtoProjectEntry {
        ProtoProjectEntry {
            project_id: project_id.to_string(),
            name: "Test Project".to_string(),
            git_url: String::new(),
            main_repo_path: "/repo".to_string(),
            daemon_instance_id: String::new(),
            main_branch_ref: String::new(),
            default_remote: String::new(),
        }
    }

    #[tokio::test]
    async fn aggregation_skips_a_peer_that_never_responds_and_returns_the_responsive_peer() {
        // Given — "mac" never answers within the per-peer timeout; "laptop" returns one project
        let forward = |peer_id: String| async move {
            if peer_id == "mac" {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            Ok(vec![a_project_entry(&format!("proj-on-{peer_id}"))])
        };

        // When — aggregating with a short per-peer timeout (outer guard: the call must not block)
        let rows = tokio::time::timeout(
            Duration::from_secs(5),
            aggregate_peer_project_entries(
                vec!["mac".to_string(), "laptop".to_string()],
                Duration::from_millis(50),
                forward,
            ),
        )
        .await
        .expect("aggregation must not block on an unresponsive peer");

        // Then — the hung peer is skipped; the responsive peer's row is present, tagged with its id
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].project_id, "proj-on-laptop");
        assert_eq!(rows[0].daemon_instance_id, "laptop");
    }

    #[tokio::test]
    async fn aggregation_skips_a_peer_whose_forward_returns_an_error() {
        // Given — "mac" errors; "laptop" returns one project
        let forward = |peer_id: String| async move {
            if peer_id == "mac" {
                return Err(tddy_rpc::Status::failed_precondition("peer offline"));
            }
            Ok(vec![a_project_entry(&format!("proj-on-{peer_id}"))])
        };

        // When
        let rows = aggregate_peer_project_entries(
            vec!["mac".to_string(), "laptop".to_string()],
            Duration::from_secs(5),
            forward,
        )
        .await;

        // Then — the erroring peer contributes nothing; the responsive peer's row remains
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].project_id, "proj-on-laptop");
        assert_eq!(rows[0].daemon_instance_id, "laptop");
    }

    // -----------------------------------------------------------------------
    // The operator's switch. Peer discovery *is* the advertisement: no connect strings means no
    // room joined, no metadata published, and nothing for a peer to find.
    // -----------------------------------------------------------------------

    /// A daemon whose LiveKit block is complete, with the operator's switch set to `enabled`.
    fn a_daemon_whose_common_room_is(enabled: bool) -> DaemonConfig {
        serde_yaml::from_str(&format!(
            "livekit:\n  enabled: {enabled}\n  url: ws://livekit.internal:7880\n  \
             api_key: devkey\n  api_secret: the-secret\n  common_room: tddy-lobby\n"
        ))
        .expect("the daemon fixture did not parse")
    }

    #[test]
    fn hands_out_the_connect_strings_when_the_common_room_is_switched_on() {
        // Given a daemon whose common room the operator switched on
        let config = a_daemon_whose_common_room_is(true);

        // When discovery asks what to connect to
        let (room, url, api_key, api_secret) = livekit_common_room_connect_strings(&config)
            .expect("a switched-on daemon must have connect strings");

        // Then it is told
        assert_eq!(
            (
                room.as_str(),
                url.as_str(),
                api_key.as_str(),
                api_secret.as_str()
            ),
            (
                "tddy-lobby",
                "ws://livekit.internal:7880",
                "devkey",
                "the-secret"
            )
        );
    }

    #[test]
    fn refuses_the_connect_strings_when_the_common_room_is_switched_off() {
        // Given the same daemon with the switch off
        let config = a_daemon_whose_common_room_is(false);

        // When discovery asks what to connect to
        let refusal = livekit_common_room_connect_strings(&config)
            .expect_err("a switched-off daemon must have nothing to connect with");

        // Then it is refused, so the registry loop assembles no discovery and publishes no
        // advertisement — a disabled daemon is not merely quiet, it is absent from the roster
        assert!(
            refusal.to_string().contains("disabled"),
            "the refusal must say the common room is disabled, not that the block is incomplete; \
             they are different operator problems. Was: {refusal}"
        );
    }
}
