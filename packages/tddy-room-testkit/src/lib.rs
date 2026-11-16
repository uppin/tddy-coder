//! An in-memory room hub for logic suites.
//!
//! A suite that proves routing or forwarding behaviour should not need a Docker container: it
//! needs two daemons in one room, each seeing the other's advertisement, and a call addressed at
//! one of them arriving stamped the way a real room stamps it. This crate is that room.
//!
//! It models the LiveKit adapter's contract, because that is the transport production uses and
//! the one `tddy-livekit-compat-tests` proves against a real server: participants join a named
//! room under an identity with metadata, a [`CommonRoomPeerRegistry`] attached to the room stays
//! synced through the same classifier the LiveKit discovery loop feeds, a service served under a
//! participant identity is reachable through [`RoomHub::client`] and through
//! [`HubPeerForwarder`] (addressed at `daemon-{instance_id}`, as the kernel's `CommonRoom`
//! addresses peers), and forwarded calls arrive with `RequestTransport::LiveKit` and the caller's
//! identity in their metadata — services decide trust on that stamp, so a converted suite must
//! keep seeing it.
//!
//! Test support only. No production crate depends on this one; suites name it from
//! `[dev-dependencies]`.

pub mod session;

pub use session::{HubSessionReach, HubSessionRooms, HUB_URL};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity;
use tddy_daemon_livekit::livekit_peer_discovery::{
    daemon_metadata_json, AdvertisedSigningKey, CommonRoomPeerRegistry, DaemonAdvertisement,
};
use tddy_host_service::multi_host::{EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::{
    RequestMetadata, RequestTransport, RpcClientTransport, RpcMessage, RpcResult, RpcService,
    Status,
};
use tokio::sync::{broadcast, mpsc, Notify};

/// How many broadcast frames a slow subscriber may lag before it is told it missed some.
const BROADCAST_DEPTH: usize = 256;
/// Frames a server-stream relay buffers between the service and the caller.
const STREAM_RELAY_DEPTH: usize = 32;

#[derive(Default)]
struct RoomState {
    /// identity → metadata, in join order.
    participants: Vec<(String, String)>,
    /// identity → the service it serves RPC with, when it serves any.
    servers: HashMap<String, Arc<dyn RpcService>>,
    /// Identities that are in the room but answer nothing: a participant whose serving loop
    /// died while its discovery participant lives on. A call to one hangs, as it does against a
    /// real room, so the caller's own forward deadline is what ends it.
    silenced: std::collections::HashSet<String>,
    /// topic → publishers of (source identity, payload).
    topics: HashMap<String, broadcast::Sender<(String, Vec<u8>)>>,
}

#[derive(Default)]
struct HubInner {
    rooms: HashMap<String, RoomState>,
}

/// Named rooms of in-memory participants. Clone to share a hub; every clone is the same hub.
#[derive(Clone, Default)]
pub struct RoomHub {
    inner: Arc<Mutex<HubInner>>,
    changed: Arc<Notify>,
}

impl RoomHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_room<R>(&self, room: &str, f: impl FnOnce(&mut RoomState) -> R) -> R {
        let mut inner = self.inner.lock().expect("room hub lock");
        f(inner.rooms.entry(room.to_string()).or_default())
    }

    fn notify_changed(&self) {
        self.changed.notify_waiters();
    }

    /// Join `room` as `identity` publishing `metadata`. Joining under an identity already in the
    /// room replaces its metadata, as a reconnect with the same identity does.
    pub fn join(&self, room: &str, identity: &str, metadata: &str) -> HubParticipant {
        self.with_room(room, |state| {
            if let Some(existing) = state.participants.iter_mut().find(|(id, _)| id == identity) {
                existing.1 = metadata.to_string();
            } else {
                state
                    .participants
                    .push((identity.to_string(), metadata.to_string()));
            }
        });
        self.notify_changed();
        HubParticipant {
            hub: self.clone(),
            room: room.to_string(),
            identity: identity.to_string(),
        }
    }

    /// Join `room` as the daemon `config` names, publishing the advertisement the LiveKit
    /// discovery loop publishes for it, signed as `signing_key` names.
    pub fn advertise_daemon(
        &self,
        room: &str,
        config: &DaemonConfig,
        signing_key: &AdvertisedSigningKey,
    ) -> HubParticipant {
        let local_id = local_instance_id_for_config(config);
        let host_id =
            tddy_daemon_livekit::livekit_peer_discovery::local_base_instance_id_for_config(config);
        let advertisement = DaemonAdvertisement {
            instance_id: local_id.clone(),
            label: format!("{local_id} (this daemon)"),
            repos_base_path: config.repos_base_path_or_default().to_string(),
            max_attachment_bytes: config.max_attachment_bytes,
            sandboxed_codebase:
                tddy_daemon_livekit::livekit_peer_discovery::sandboxed_codebase_support(),
            signing_key_id: signing_key.key_id.clone(),
            signing_public_key: signing_key.public_key.clone(),
        };
        let metadata = daemon_metadata_json(&advertisement, &host_id)
            .expect("a daemon advertisement serializes");
        self.join(room, &local_id, &metadata)
    }

    /// The room's participants as `(identity, metadata)`, in join order.
    pub fn participants(&self, room: &str) -> Vec<(String, String)> {
        self.with_room(room, |state| state.participants.clone())
    }

    /// Remove `identity` from `room` — the participant and anything it served — as a
    /// [`HubParticipant::leave`] by name, for closing a room whose handles are gone.
    pub fn leave(&self, room: &str, identity: &str) {
        self.with_room(room, |state| {
            state.participants.retain(|(id, _)| id != identity);
            state.servers.remove(identity);
            state.silenced.remove(identity);
        });
        self.notify_changed();
    }

    /// Wait until `identity` is in `room`, or `timeout` passes.
    pub async fn wait_for_participant(
        &self,
        room: &str,
        identity: &str,
        timeout: Duration,
    ) -> Result<(), Status> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.participants(room).iter().any(|(id, _)| id == identity) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Status::deadline_exceeded(format!(
                    "participant {identity} did not join room {room}"
                )));
            }
            tokio::select! {
                _ = self.changed.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
        }
    }

    /// Keep `registry` synced with `room`'s participants, classified for the daemon
    /// `local_instance_id`, from now until the returned handle is dropped.
    ///
    /// This is the hub's stand-in for the discovery loop's registry half: the same
    /// `sync_from_participant_fields` the LiveKit loop's room snapshot feeds, driven here by the
    /// hub's change notifications.
    pub fn attach_registry(
        &self,
        room: &str,
        local_instance_id: &str,
        registry: Arc<CommonRoomPeerRegistry>,
    ) -> tokio::task::JoinHandle<()> {
        let hub = self.clone();
        let room = room.to_string();
        let local_instance_id = local_instance_id.to_string();
        tokio::spawn(async move {
            loop {
                registry.sync_from_participant_fields(hub.participants(&room), &local_instance_id);
                tokio::select! {
                    _ = hub.changed.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        })
    }

    /// Serve `service` under `participant`'s identity in its room, until the participant leaves.
    pub fn serve(&self, participant: &HubParticipant, service: Arc<dyn RpcService>) {
        self.with_room(&participant.room, |state| {
            state.servers.insert(participant.identity.clone(), service);
        });
    }

    fn server(&self, room: &str, identity: &str) -> Option<Arc<dyn RpcService>> {
        self.with_room(room, |state| state.servers.get(identity).cloned())
    }

    fn is_silenced(&self, room: &str, identity: &str) -> bool {
        self.with_room(room, |state| state.silenced.contains(identity))
    }

    /// An RPC client addressed at `target_identity` in `room`, calling as `from_identity`.
    pub fn client(&self, room: &str, from_identity: &str, target_identity: &str) -> HubClient {
        HubClient {
            hub: self.clone(),
            room: room.to_string(),
            from_identity: from_identity.to_string(),
            target_identity: target_identity.to_string(),
        }
    }

    /// Publish `payload` on `topic` in `room`, from `participant`.
    pub fn publish(&self, participant: &HubParticipant, topic: &str, payload: Vec<u8>) {
        let sender = self.with_room(&participant.room, |state| {
            state
                .topics
                .entry(topic.to_string())
                .or_insert_with(|| broadcast::channel(BROADCAST_DEPTH).0)
                .clone()
        });
        // No subscriber yet is the ordinary case, not an error: a broadcast is fire-and-forget.
        let _ = sender.send((participant.identity.clone(), payload));
    }

    /// Subscribe to `topic` in `room`: `(source identity, payload)` per frame.
    pub fn subscribe(&self, room: &str, topic: &str) -> broadcast::Receiver<(String, Vec<u8>)> {
        self.with_room(room, |state| {
            state
                .topics
                .entry(topic.to_string())
                .or_insert_with(|| broadcast::channel(BROADCAST_DEPTH).0)
                .subscribe()
        })
    }
}

/// One participant in one room of a [`RoomHub`]. Leaving is explicit ([`Self::leave`]) so a test
/// that holds a participant past a disconnect it is simulating says so.
pub struct HubParticipant {
    hub: RoomHub,
    room: String,
    identity: String,
}

impl HubParticipant {
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn room(&self) -> &str {
        &self.room
    }

    /// Replace this participant's published metadata; watchers and attached registries follow.
    pub fn set_metadata(&self, metadata: &str) {
        self.hub.with_room(&self.room, |state| {
            if let Some(existing) = state
                .participants
                .iter_mut()
                .find(|(id, _)| id == &self.identity)
            {
                existing.1 = metadata.to_string();
            }
        });
        self.hub.notify_changed();
    }

    /// Serve `service` under this participant's identity. See [`RoomHub::serve`].
    pub fn serve(&self, service: Arc<dyn RpcService>) {
        self.hub.serve(self, service);
    }

    /// Stop (or resume) answering calls, while staying in the room: the model of a participant
    /// whose serving loop died. Calls to a silenced participant never return; the caller's
    /// forward deadline ends them, as it does against a real room.
    pub fn set_silenced(&self, silenced: bool) {
        self.hub.with_room(&self.room, |state| {
            if silenced {
                state.silenced.insert(self.identity.clone());
            } else {
                state.silenced.remove(&self.identity);
            }
        });
    }

    /// Publish on `topic` in this participant's room. See [`RoomHub::publish`].
    pub fn publish(&self, topic: &str, payload: Vec<u8>) {
        self.hub.publish(self, topic, payload);
    }

    /// Leave the room: the participant and anything it served are gone, and watchers follow.
    pub fn leave(&self) {
        self.hub.with_room(&self.room, |state| {
            state.participants.retain(|(id, _)| id != &self.identity);
            state.servers.remove(&self.identity);
            state.silenced.remove(&self.identity);
        });
        self.hub.notify_changed();
    }
}

impl Drop for HubParticipant {
    fn drop(&mut self) {
        self.leave();
    }
}

/// An RPC client addressed at one participant of one room, calling as another.
pub struct HubClient {
    hub: RoomHub,
    room: String,
    from_identity: String,
    target_identity: String,
}

impl HubClient {
    fn message(&self, payload: Vec<u8>) -> RpcMessage {
        RpcMessage::new(
            payload,
            RequestMetadata::over(RequestTransport::LiveKit)
                .with_sender_identity(Some(self.from_identity.clone())),
        )
    }

    async fn dispatch(
        &self,
        service: &str,
        method: &str,
        payload: Vec<u8>,
    ) -> Result<RpcResult, Status> {
        if self.hub.is_silenced(&self.room, &self.target_identity) {
            // A participant that stopped answering: the frame is delivered and nothing comes
            // back, ever. The caller's deadline is the only end to this call.
            std::future::pending::<()>().await;
        }
        let server = self
            .hub
            .server(&self.room, &self.target_identity)
            .ok_or_else(|| {
                Status::unavailable(format!(
                    "participant {} is not in room {}",
                    self.target_identity, self.room
                ))
            })?;
        Ok(server
            .handle_rpc(service, method, &self.message(payload))
            .await)
    }
}

#[async_trait::async_trait]
impl RpcClientTransport for HubClient {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        match self.dispatch(service, method, request_bytes).await? {
            RpcResult::Unary(result) => result,
            RpcResult::ServerStream(Ok(mut frames)) => {
                // A unary call to a streaming method collects its frames, as the response loop
                // of a shared client does when the caller asked for one answer.
                let mut collected = Vec::new();
                while let Some(frame) = frames.recv().await {
                    collected.extend_from_slice(&frame?);
                }
                Ok(collected)
            }
            RpcResult::ServerStream(Err(status)) => Err(status),
        }
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        match self.dispatch(service, method, request_bytes).await? {
            RpcResult::ServerStream(result) => result,
            RpcResult::Unary(Ok(bytes)) => {
                let (tx, rx) = mpsc::channel(1);
                let _ = tx.send(Ok(bytes)).await;
                Ok(rx)
            }
            RpcResult::Unary(Err(status)) => Err(status),
        }
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented(
            "the in-memory room hub carries unary and server-streaming calls; a client-streaming logic suite belongs on the stdio/IPC adapter",
        ))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(
            "the in-memory room hub carries unary and server-streaming calls; a bidi logic suite belongs on the stdio/IPC adapter",
        ))
    }
}

/// Forwards through a hub room, addressed the way the kernel's `CommonRoom` addresses peers:
/// a call for `peer_id` goes to the participant `daemon-{peer_id}`.
pub struct HubPeerForwarder {
    hub: RoomHub,
    room: String,
    from_identity: String,
}

impl HubPeerForwarder {
    pub fn new(
        hub: RoomHub,
        room: impl Into<String>,
        from_identity: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            hub,
            room: room.into(),
            from_identity: from_identity.into(),
        })
    }
}

#[async_trait::async_trait]
impl tddy_daemon_extensions::PeerForwarder for HubPeerForwarder {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.hub
            .client(
                &self.room,
                &self.from_identity,
                &daemon_rpc_identity(peer_id),
            )
            .call_unary(service, method, body)
            .await
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let mut frames = self
            .hub
            .client(
                &self.room,
                &self.from_identity,
                &daemon_rpc_identity(peer_id),
            )
            .call_server_stream(service, method, body)
            .await?;
        let (tx, rx) = mpsc::channel(STREAM_RELAY_DEPTH);
        tokio::spawn(async move {
            loop {
                let frame = tokio::select! {
                    frame = frames.recv() => frame,
                    _ = tx.closed() => break,
                };
                let Some(frame) = frame else { break };
                if tx.send(frame).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}

/// An [`EligibleDaemonSource`] over a hub room: this daemon plus every daemon an attached
/// registry can see in it, merged the way the LiveKit source merges them.
pub struct HubEligibleDaemonSource {
    config: Arc<DaemonConfig>,
    registry: Arc<CommonRoomPeerRegistry>,
    common_room: tddy_daemon_kernel::peer_forwarding::CommonRoom,
}

impl HubEligibleDaemonSource {
    pub fn new(
        config: Arc<DaemonConfig>,
        registry: Arc<CommonRoomPeerRegistry>,
        common_room: tddy_daemon_kernel::peer_forwarding::CommonRoom,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            registry,
            common_room,
        })
    }

    /// The forwarding room this source routes through, for a `PeerRouting` over the same room.
    pub fn common_room(&self) -> tddy_daemon_kernel::peer_forwarding::CommonRoom {
        self.common_room.clone()
    }
}

impl EligibleDaemonSource for HubEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        let local = tddy_host_service::multi_host::eligible_daemon_entry_for(
            tddy_host_service::multi_host::DaemonInstanceId(local_instance_id_for_config(
                &self.config,
            )),
        );
        let remotes = self.registry.snapshot_remotes();
        tddy_daemon_livekit::livekit_peer_discovery::merge_discovered_peers_ordered(local, remotes)
            .unwrap_or_else(|e| {
                log::warn!(
                    "merge_discovered_peers_ordered failed: {} — returning local only",
                    e
                );
                vec![tddy_host_service::multi_host::eligible_daemon_entry_for(
                    tddy_host_service::multi_host::DaemonInstanceId(local_instance_id_for_config(
                        &self.config,
                    )),
                )]
            })
    }
}
