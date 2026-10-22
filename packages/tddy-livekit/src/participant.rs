//! LiveKit room participant that serves RPC over the data channel.
//!
//! Dispatch/multiplexing (bidi sessions, client-streaming message accumulation, per-request-id
//! correlation) is delegated to [`tddy_rpc::server_engine::ServerEngine`] — the same engine
//! `tddy-stdio` uses. What's left here is genuinely LiveKit-specific: resolving which participant
//! identity a response should go to (multiple remotes may share a room), buffering incoming bytes
//! until a remote has connected, and publishing responses in a way that survives reconnection.

pub(crate) mod publisher;
mod serve;

use livekit::prelude::*;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex};

use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::RequestTransport;

use crate::projects_registry;
use crate::proto::{RpcRequest, RpcResponse};
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

use crate::participant::publisher::spawn_response_drain;
use crate::participant::publisher::spawn_response_drain_reconnectable;
use crate::participant::publisher::SharedPublisher;
/// Shallow-merge two JSON objects for [`LocalParticipant::set_metadata`]; the helper lives in
/// `tddy-changeset` (re-exported by `tddy-core`) so a consumer need not depend on this crate to reach it.
pub use tddy_changeset::session_participant_metadata::merge_participant_metadata_json;

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

/// A LiveKit room participant that routes RPC traffic to a [`ServerEngine`].
pub struct LiveKitParticipant<S: crate::bridge::RpcService> {
    room: Room,
    server: Arc<ServerEngine<S>>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    /// Sender half of the channel drained by the response-drain task (see [`spawn_response_drain`]
    /// / [`spawn_response_drain_reconnectable`]); cloned into every [`ServerEngine::on_request`]
    /// call so handler tasks can publish responses without touching the transport directly.
    outgoing_tx: mpsc::Sender<(String, RpcResponse)>,
    /// Payloads received with participant=None before any remote joined (race with ParticipantConnected).
    pending_data: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// Poll this path for an `https://` authorize URL (Codex `BROWSER` hook) and publish **metadata
    /// only** for UIs. Includes `callback_port` / `state` when
    /// derivable from the URL so the desktop relay matches the terminal-driven metadata shape.
    codex_oauth_watch: Option<PathBuf>,
    /// When set, publish [`OWNED_PROJECT_COUNT_METADATA_KEY`] from the registry at this directory (e.g. daemon project store).
    projects_registry_dir: Option<PathBuf>,
    /// Coordinates read–merge–write so OAuth, project count, and watch-channel updates do not clobber each other.
    metadata_publish_lock: Arc<Mutex<()>>,
}

/// Outgoing-response channel capacity. Bounded so a stalled publish path applies backpressure to
/// handler tasks rather than letting memory grow unbounded.
const OUTGOING_QUEUE_CAPACITY: usize = 256;

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
        // Whoever the room admitted sent it — stamped here, never read from the envelope.
        let server = Arc::new(ServerEngine::new(service, RequestTransport::LiveKit));
        let (room, events) = Room::connect(url, token, room_options).await?;
        log::info!(
            "[echo_server] LiveKitParticipant connected, identity={:?}",
            room.local_participant().identity()
        );
        let (outgoing_tx, outgoing_rx) = mpsc::channel(OUTGOING_QUEUE_CAPACITY);
        spawn_response_drain(outgoing_rx, room.local_participant().clone());
        Ok(Self {
            room,
            server,
            events,
            outgoing_tx,
            pending_data: Arc::new(Mutex::new(VecDeque::new())),
            codex_oauth_watch,
            projects_registry_dir,
            metadata_publish_lock: Arc::new(Mutex::new(())),
        })
    }

    /// Connect to a LiveKit room, sharing the [`ServerEngine`] and outgoing channel across
    /// reconnection cycles. The `SharedPublisher` is updated with the new `LocalParticipant` so
    /// that the (separately, long-lived) response-drain task publishes through the new room.
    #[allow(clippy::too_many_arguments)]
    async fn connect_for_reconnect(
        url: &str,
        token: &str,
        server: Arc<ServerEngine<S>>,
        room_options: RoomOptions,
        outgoing_tx: mpsc::Sender<(String, RpcResponse)>,
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
            server,
            events,
            outgoing_tx,
            pending_data: Arc::new(Mutex::new(VecDeque::new())),
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
        match Self::join(
            url,
            token_generator,
            service,
            room_options,
            metadata_watch,
            codex_oauth_watch,
            projects_registry_dir,
        )
        .await
        {
            Ok(joined) => joined.serve(shutdown).await,
            Err(e) => log::error!("LiveKit connect failed: {e:#}"),
        }
    }

    /// Join the room now and hand back a connection that is **already a participant**, ready to be
    /// served with [`JoinedParticipant::serve`].
    ///
    /// [`Self::run_with_reconnect`] joins inside the task its caller spawns, which is right for a
    /// daemon advertising itself in a lobby and wrong for one that has to be provably in a room
    /// before it tells anyone the room exists — a session room's "the facilitating daemon is the
    /// first participant" property is a consequence of *awaiting* this
    /// (`docs/ft/daemon/session-room.md` FR2), not a race it hopes to win.
    #[allow(clippy::too_many_arguments)]
    pub async fn join(
        url: &str,
        token_generator: &TokenGenerator,
        service: S,
        room_options: RoomOptions,
        metadata_watch: Option<watch::Receiver<String>>,
        codex_oauth_watch: Option<PathBuf>,
        projects_registry_dir: Option<PathBuf>,
    ) -> anyhow::Result<JoinedParticipant<S>> {
        let server = Arc::new(ServerEngine::new(service, RequestTransport::LiveKit));
        let shared_publisher = SharedPublisher::new();
        let (outgoing_tx, outgoing_rx) = mpsc::channel(OUTGOING_QUEUE_CAPACITY);
        // Long-lived: survives every reconnect, since `shared_publisher` (not a
        // `LocalParticipant` snapshot) retries against whatever room is current.
        spawn_response_drain_reconnectable(outgoing_rx, shared_publisher.clone());

        let token = token_generator
            .generate()
            .map_err(|e| anyhow::anyhow!("token generation failed: {e}"))?;
        let participant = Self::connect_for_reconnect(
            url,
            &token,
            server,
            room_options,
            outgoing_tx,
            shared_publisher.clone(),
            codex_oauth_watch,
            projects_registry_dir,
        )
        .await
        .map_err(|e| anyhow::anyhow!("joining {url}: {e}"))?;
        log::info!("READY");
        log::info!(
            "[livekit] participant running (jwt_ttl={:?}, no timer-driven reconnect)",
            token_generator.ttl()
        );

        Ok(JoinedParticipant {
            participant,
            shared_publisher,
            metadata_watch,
        })
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

/// A connection that has joined its room but is not serving yet — the gap
/// [`LiveKitParticipant::join`] hands to its caller.
///
/// Two things need that gap: proving membership before announcing the room, and taking a
/// [`BroadcastPublisher`] on the connection *before* [`LiveKitParticipant::run`] consumes it.
pub struct JoinedParticipant<S: crate::bridge::RpcService> {
    participant: LiveKitParticipant<S>,
    shared_publisher: SharedPublisher,
    metadata_watch: Option<watch::Receiver<String>>,
}

impl<S: crate::bridge::RpcService> JoinedParticipant<S> {
    /// The identity this connection joined under.
    pub fn identity(&self) -> String {
        self.participant
            .room()
            .local_participant()
            .identity()
            .to_string()
    }

    /// A publisher for room-wide broadcasts on `topic`, over this connection.
    ///
    /// Follows the connection across reconnects rather than pinning the participant it has right
    /// now, so a publisher held for the life of a room keeps reaching the room (see
    /// [`crate::BroadcastPublisher`]).
    pub fn broadcast_on(&self, topic: impl Into<String>) -> crate::broadcast::BroadcastPublisher {
        crate::broadcast::BroadcastPublisher::for_connection(self.shared_publisher.clone(), topic)
    }

    /// Serve RPC in the room until the connection ends or `shutdown` is set.
    ///
    /// Returning means this participant is **no longer in the room**: the caller owns saying so.
    /// Nothing here reconnects on its own — the SDK handles connection health and server-driven
    /// token refresh on the signalling channel, and a stream that ended anyway is a fact the caller
    /// has to act on rather than one to paper over.
    pub async fn serve(self, shutdown: Arc<AtomicBool>) {
        let metadata_task = if let Some(rx) = self.metadata_watch {
            let local = self.participant.room().local_participant().clone();
            let lock = self.participant.metadata_publish_lock.clone();
            Some(spawn_local_participant_metadata_watcher(rx, local, lock))
        } else {
            None
        };

        tokio::select! {
            _ = self.participant.run() => {
                log::info!("[livekit] participant.run() returned (disconnected)");
            }
            _ = async {
                while !shutdown.load(Ordering::Relaxed) {
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
    use serde_json::Value;

    /// The connection these test requests were made on.
    const A_CLIENT_EPOCH: u32 = 0x5f3a_91c2;
    use crate::proto::CallMetadata;

    #[test]
    fn resolve_response_identity_uses_sender_identity_from_request_when_present() {
        // Given a request with sender_identity set and multiple remote participants
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
            client_epoch: A_CLIENT_EPOCH,
        };
        let event_participant = Some("client2".to_string().into());
        let remote_identities = vec!["client1".to_string().into(), "client2".to_string().into()];

        // When resolving the response identity
        let result = resolve_response_identity(&request, event_participant, &remote_identities);

        // Then it uses sender_identity from the request (not event participant or sole remote)
        assert_eq!(
            result.as_ref().map(|p| p.as_str()),
            Some("client1"),
            "response must be sent to sender_identity from request, not event participant"
        );
    }

    #[test]
    fn merge_participant_metadata_json_retains_baseline_keys_on_partial_update() {
        // Given existing metadata with multiple keys
        let baseline = r#"{"codex_oauth":{"pending":false},"legacy":1}"#;
        let update = format!(r#"{{"{key}":9}}"#, key = OWNED_PROJECT_COUNT_METADATA_KEY);

        // When merging with a partial update (only owned_project_count)
        let merged = merge_participant_metadata_json(baseline, &update).expect("merge");
        let v: Value = serde_json::from_str(&merged).expect("merged JSON");

        // Then baseline-only keys are retained and the new key is written
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
