//! The daemon's common-room connection, owned by a supervisor that can be reconfigured while the
//! daemon runs.
//!
//! A daemon is present in its common room twice — the RPC participant serving the roster under
//! `daemon-{instance}`, and the discovery participant publishing this daemon's advertisement — and
//! both are defined by the LiveKit block. When an operator edits that block through
//! [`crate::daemon_config_service`], neither can stay as it was: the room they are in is no longer
//! the room the daemon is configured for.
//!
//! So the connection lives here, behind a [`watch`] channel. [`SupervisedCommonRoom::reconfigure`]
//! publishes the room the daemon should now be in; [`CommonRoomSupervisorTask`] owns whatever is
//! running and reconciles it. The order is always **down, then up**: the running connection is torn
//! down before anything replaces it, so a configuration that cannot be joined leaves the daemon
//! disconnected rather than quietly on the room it was told to leave.
//!
//! What joining actually involves is behind [`CommonRoomConnector`], so the supervisor's lifecycle
//! is exercised without a LiveKit server — and so [`crate::runtime`] stays assembly: it builds the
//! supervisor and hands the task to its host, which starts it with the rest.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::config::{DaemonConfig, LiveKitConfig};
use crate::daemon_config_service::CommonRoomSupervisor;

/// How long a departing RPC participant is given to leave its room before it is dropped where it
/// stands. It checks its shutdown flag between turns of its event loop (every 100 ms), so this is
/// two orders of magnitude of headroom — it exists so a participant wedged in a publish cannot hold
/// the next connection back for ever.
const PARTICIPANT_LEAVE_GRACE: Duration = Duration::from_secs(2);

/// A LiveKit block proven to name a room this daemon can join: server URL, API key, API secret and
/// room name all present and non-blank.
///
/// A partially configured block is not a room, so it never becomes one of these — which is what
/// makes "connect to whatever the operator saved" a total function rather than a runtime surprise.
#[derive(Clone, Debug)]
pub struct CommonRoomTarget {
    livekit: LiveKitConfig,
    url: String,
    api_key: String,
    api_secret: String,
    room: String,
}

impl CommonRoomTarget {
    /// The room `livekit` names, or `None` when it names no joinable one.
    pub fn from_livekit(livekit: Option<&LiveKitConfig>) -> Option<Self> {
        let livekit = livekit?;
        let non_blank = |value: Option<&String>| {
            value
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Some(Self {
            url: non_blank(livekit.url.as_ref())?,
            api_key: non_blank(livekit.api_key.as_ref())?,
            api_secret: non_blank(livekit.api_secret.as_ref())?,
            room: non_blank(livekit.common_room.as_ref())?,
            livekit: livekit.clone(),
        })
    }

    /// The LiveKit server to join.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The room to join on it.
    pub fn room(&self) -> &str {
        &self.room
    }

    /// The API key a join token is minted with.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// The API secret a join token is signed with.
    pub fn api_secret(&self) -> &str {
        &self.api_secret
    }

    /// The whole block, for the parts of the daemon that read more of it than the four connection
    /// strings — peer discovery reads the metadata publish budget from it, for one.
    pub fn livekit(&self) -> &LiveKitConfig {
        &self.livekit
    }
}

/// Brings up a common-room participant for a target.
///
/// The supervisor never learns what joining a room involves, which is what lets its lifecycle be
/// driven — and asserted on — without a LiveKit server.
#[async_trait]
pub trait CommonRoomConnector: Send + Sync + 'static {
    /// Join the room `target` names and start everything this daemon runs on it.
    async fn connect(&self, target: CommonRoomTarget) -> Box<dyn ConnectedCommonRoom>;
}

/// A common-room connection that is running.
#[async_trait]
pub trait ConnectedCommonRoom: Send + Sync {
    /// Leave the room and stop everything this connection started. The supervisor awaits this
    /// before it connects anything else, so a reconnect never has two participants claiming the
    /// same `daemon-{instance}` identity at once.
    async fn disconnect(self: Box<Self>);
}

/// The handle the rest of the daemon reconfigures the common room through.
///
/// Holding it is what keeps the connection alive: when the last handle is dropped — the daemon's
/// roster going away with it — the supervising task leaves the room and ends.
pub struct SupervisedCommonRoom {
    desired: watch::Sender<Option<CommonRoomTarget>>,
}

impl SupervisedCommonRoom {
    /// A supervisor whose connection starts as the room `livekit` names, or disconnected when it
    /// names none.
    pub fn new(livekit: Option<&LiveKitConfig>) -> Self {
        let (desired, _) = watch::channel(CommonRoomTarget::from_livekit(livekit));
        Self { desired }
    }

    /// The task that owns the connection, for the host to start. Nothing is joined until it runs.
    pub fn task(&self, connector: Arc<dyn CommonRoomConnector>) -> CommonRoomSupervisorTask {
        CommonRoomSupervisorTask {
            desired: self.desired.subscribe(),
            connector,
        }
    }
}

impl CommonRoomSupervisor for SupervisedCommonRoom {
    fn reconfigure(&self, livekit: Option<LiveKitConfig>) {
        let target = CommonRoomTarget::from_livekit(livekit.as_ref());
        if target.is_none() && livekit.is_some() {
            log::warn!(
                target: "tddy_daemon::common_room",
                "the updated livekit block names no joinable common room (url, api_key, api_secret \
                 and common_room are all required) — this daemon will leave the room it is in and \
                 stay disconnected"
            );
        }
        // The supervising task owns the teardown and the join; an RPC handler answering an operator
        // must not block on a LiveKit round-trip to do it.
        self.desired.send_replace(target);
    }
}

/// Owns the running common-room connection and reconciles it with the configured one.
pub struct CommonRoomSupervisorTask {
    desired: watch::Receiver<Option<CommonRoomTarget>>,
    connector: Arc<dyn CommonRoomConnector>,
}

impl CommonRoomSupervisorTask {
    /// Join the configured room, and rejoin whenever the configuration names a different one.
    /// Returns once the last [`SupervisedCommonRoom`] is dropped, leaving no connection behind.
    pub async fn run(mut self) {
        let mut connected: Option<Box<dyn ConnectedCommonRoom>> = None;
        loop {
            let target = self.desired.borrow_and_update().clone();

            // Down, then up — never the other way round. Whatever becomes of the new connection,
            // the daemon is not left on the room it was told to leave while reporting the one it
            // was told to join.
            if let Some(running) = connected.take() {
                running.disconnect().await;
            }
            match target {
                Some(target) => {
                    log::info!(
                        target: "tddy_daemon::common_room",
                        "joining common room {} at {}",
                        target.room(),
                        target.url()
                    );
                    connected = Some(self.connector.connect(target).await);
                }
                None => log::info!(
                    target: "tddy_daemon::common_room",
                    "no joinable common room is configured — this daemon stays disconnected"
                ),
            }

            if self.desired.changed().await.is_err() {
                break;
            }
        }
        // The daemon dropped its supervisor: it is shutting down, and its participants leave with it.
        if let Some(running) = connected {
            running.disconnect().await;
        }
    }
}

/// Peer discovery's shared handles: the registry the roster's eligible-daemon source reads, and the
/// room slot peer forwarding publishes through.
///
/// The same two live across every reconnect — the roster was built holding them, and a reconnect
/// that replaced them would leave every forwarding path pointing at a room nobody is in.
#[derive(Clone)]
pub struct PeerDiscoveryHandles {
    pub registry: Arc<crate::livekit_peer_discovery::CommonRoomPeerRegistry>,
    pub room_slot: Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
}

/// The real thing: joins the common room as this daemon, serving its RPC roster and running peer
/// discovery.
pub struct DaemonCommonRoomConnector {
    /// Everything but the LiveKit block, which the target carries.
    config: DaemonConfig,
    /// The RPC roster served on the room.
    entries: Vec<tddy_rpc::ServiceEntry>,
    /// `None` when this daemon assembled no peer discovery — see [`Self::connect`].
    peer_discovery: Option<PeerDiscoveryHandles>,
}

impl DaemonCommonRoomConnector {
    pub fn new(
        config: DaemonConfig,
        entries: Vec<tddy_rpc::ServiceEntry>,
        peer_discovery: Option<PeerDiscoveryHandles>,
    ) -> Self {
        Self {
            config,
            entries,
            peer_discovery,
        }
    }
}

/// One `ServiceEntry` per entry in `entries`, sharing the same service implementations: a roster is
/// a list of `Arc`s, and every transport it is served on needs its own list of them.
pub fn cloned_entries(entries: &[tddy_rpc::ServiceEntry]) -> Vec<tddy_rpc::ServiceEntry> {
    entries
        .iter()
        .map(|entry| tddy_rpc::ServiceEntry {
            name: entry.name,
            service: entry.service.clone(),
        })
        .collect()
}

#[async_trait]
impl CommonRoomConnector for DaemonCommonRoomConnector {
    async fn connect(&self, target: CommonRoomTarget) -> Box<dyn ConnectedCommonRoom> {
        // The block the supervisor was told to join, so discovery and the RPC participant reach the
        // same room. Every other field is the one this daemon started with — a change to any of
        // them is reported as restart-required rather than applied here.
        let mut config = self.config.clone();
        config.livekit = Some(target.livekit().clone());
        let config = Arc::new(config);

        let discovery = match &self.peer_discovery {
            Some(handles) => Some(
                crate::livekit_peer_discovery::spawn_common_room_discovery_loop(
                    config.clone(),
                    handles.registry.clone(),
                    handles.room_slot.clone(),
                ),
            ),
            None => {
                // TODO: peer discovery is assembled at startup and only when the daemon starts with
                // a joinable common room, so a daemon that gains one at runtime serves its roster
                // on the room without discovering the other daemons in it until it is restarted.
                log::warn!(
                    target: "tddy_daemon::common_room",
                    "serving the RPC roster on common room {} with no peer discovery: this daemon \
                     assembled none at startup. Restart it to discover the other daemons in the room.",
                    target.room()
                );
                None
            }
        };

        // The identity the roster is addressed at, unchanged across reconnects: it is derived from
        // this daemon's instance id, which a LiveKit edit does not touch. Peers that resolved it
        // before the reconnect still reach this daemon after it.
        let identity = crate::livekit_peer_discovery::daemon_rpc_identity(
            &crate::livekit_peer_discovery::local_instance_id_for_config(&config),
        );
        let token_generator = tddy_livekit::TokenGenerator::new(
            target.api_key().to_string(),
            target.api_secret().to_string(),
            target.room().to_string(),
            identity,
            Duration::from_secs(tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS),
        );
        let roster = tddy_rpc::MultiRpcService::new(cloned_entries(&self.entries));
        let shutdown = Arc::new(AtomicBool::new(false));
        let url = target.url().to_string();
        let participant = tokio::spawn({
            let shutdown = shutdown.clone();
            async move {
                tddy_livekit::LiveKitParticipant::run_with_reconnect(
                    &url,
                    &token_generator,
                    roster,
                    Default::default(),
                    shutdown,
                    None,
                    None,
                )
                .await;
            }
        });

        Box::new(RunningCommonRoom {
            participant,
            participant_shutdown: shutdown,
            discovery,
            peer_discovery: self.peer_discovery.clone(),
        })
    }
}

/// This daemon's live presence in a common room: the RPC participant, the discovery loop, and the
/// handles they fill.
struct RunningCommonRoom {
    participant: JoinHandle<()>,
    participant_shutdown: Arc<AtomicBool>,
    discovery: Option<JoinHandle<()>>,
    peer_discovery: Option<PeerDiscoveryHandles>,
}

#[async_trait]
impl ConnectedCommonRoom for RunningCommonRoom {
    async fn disconnect(self: Box<Self>) {
        // Discovery reconnects for ever by design, so it is stopped before its room is closed —
        // otherwise it would simply rejoin the room this daemon is leaving.
        if let Some(discovery) = self.discovery {
            discovery.abort();
        }
        if let Some(handles) = self.peer_discovery {
            let room = handles.room_slot.write().await.take();
            if let Some(room) = room {
                if let Err(e) = room.close().await {
                    log::warn!(
                        target: "tddy_daemon::common_room",
                        "closing the discovery participant's room: {e}"
                    );
                }
            }
            // The peers in the room being left are not the peers in the room being joined.
            handles.registry.clear();
        }

        self.participant_shutdown.store(true, Ordering::Relaxed);
        let mut participant = self.participant;
        if tokio::time::timeout(PARTICIPANT_LEAVE_GRACE, &mut participant)
            .await
            .is_err()
        {
            log::warn!(
                target: "tddy_daemon::common_room",
                "the RPC participant did not leave its room within {PARTICIPANT_LEAVE_GRACE:?} — dropping it"
            );
            participant.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    /// How long a test waits for the supervisor to act. It is a bound, not a duration: the
    /// supervisor's steps are awaited signals, and this exists so a failure says what never
    /// happened instead of hanging.
    const A_BOUNDED_WAIT: Duration = Duration::from_secs(2);

    const LOBBY_A: &str = "ws://lobby-a.example:7880";
    const LOBBY_B: &str = "ws://lobby-b.example:7880";
    const LOBBY_C: &str = "ws://lobby-c.example:7880";
    const THE_LOBBY: &str = "tddy-lobby";

    /// What the supervisor did to this daemon's presence in a common room.
    #[derive(Debug, PartialEq, Eq)]
    enum RoomLifecycle {
        Joined(String),
        Left(String),
    }

    /// A common room as the daemon is present in it: a server and a room on it are together the
    /// connection, so a change to either is a different room to be in.
    fn the_room(url: &str, room: &str) -> String {
        format!("{url}/{room}")
    }

    struct LiveKitBlockBuilder {
        livekit: LiveKitConfig,
    }

    /// A LiveKit block naming a joinable common room.
    fn a_livekit_block() -> LiveKitBlockBuilder {
        LiveKitBlockBuilder {
            livekit: LiveKitConfig {
                url: Some(LOBBY_A.to_string()),
                api_key: Some("devkey".to_string()),
                api_secret: Some("the-secret".to_string()),
                common_room: Some(THE_LOBBY.to_string()),
                ..Default::default()
            },
        }
    }

    impl LiveKitBlockBuilder {
        fn with_url(mut self, url: &str) -> Self {
            self.livekit.url = Some(url.to_string());
            self
        }

        fn with_room(mut self, room: &str) -> Self {
            self.livekit.common_room = Some(room.to_string());
            self
        }

        fn with_no_url(mut self) -> Self {
            self.livekit.url = None;
            self
        }

        fn with_no_api_key(mut self) -> Self {
            self.livekit.api_key = None;
            self
        }

        fn with_no_api_secret(mut self) -> Self {
            self.livekit.api_secret = None;
            self
        }

        fn with_no_room(mut self) -> Self {
            self.livekit.common_room = None;
            self
        }

        fn with_a_blank_room(mut self) -> Self {
            self.livekit.common_room = Some("   ".to_string());
            self
        }

        fn build(self) -> LiveKitConfig {
            self.livekit
        }
    }

    /// A common room that records this daemon's comings and goings instead of joining LiveKit.
    struct RecordingRooms {
        lifecycle: mpsc::UnboundedSender<RoomLifecycle>,
    }

    #[async_trait]
    impl CommonRoomConnector for RecordingRooms {
        async fn connect(&self, target: CommonRoomTarget) -> Box<dyn ConnectedCommonRoom> {
            let room = the_room(target.url(), target.room());
            self.lifecycle
                .send(RoomLifecycle::Joined(room.clone()))
                .expect("the test stopped watching the common room");
            Box::new(RecordedRoom {
                room,
                lifecycle: self.lifecycle.clone(),
            })
        }
    }

    struct RecordedRoom {
        room: String,
        lifecycle: mpsc::UnboundedSender<RoomLifecycle>,
    }

    #[async_trait]
    impl ConnectedCommonRoom for RecordedRoom {
        async fn disconnect(self: Box<Self>) {
            self.lifecycle
                .send(RoomLifecycle::Left(self.room))
                .expect("the test stopped watching the common room");
        }
    }

    /// A supervisor with its task already running, and the room lifecycle it drives.
    struct SupervisedDaemon {
        supervisor: Option<SupervisedCommonRoom>,
        lifecycle: mpsc::UnboundedReceiver<RoomLifecycle>,
        task: JoinHandle<()>,
    }

    fn a_daemon_supervising(livekit: Option<LiveKitConfig>) -> SupervisedDaemon {
        let (sender, lifecycle) = mpsc::unbounded_channel();
        let supervisor = SupervisedCommonRoom::new(livekit.as_ref());
        let task = tokio::spawn(
            supervisor
                .task(Arc::new(RecordingRooms { lifecycle: sender }))
                .run(),
        );
        SupervisedDaemon {
            supervisor: Some(supervisor),
            lifecycle,
            task,
        }
    }

    impl SupervisedDaemon {
        fn reconfigure(&self, livekit: Option<LiveKitConfig>) {
            self.supervisor
                .as_ref()
                .expect("this daemon has already dropped its supervisor")
                .reconfigure(livekit);
        }

        /// The next thing the supervisor did to the common room.
        async fn next_room_lifecycle(&mut self) -> RoomLifecycle {
            timeout(A_BOUNDED_WAIT, self.lifecycle.recv())
                .await
                .expect("the supervisor did nothing to the common room")
                .expect("the supervisor ended without touching the common room again")
        }

        /// This daemon shutting down: the roster holding the supervisor goes away.
        fn drop_the_supervisor(&mut self) {
            self.supervisor = None;
        }

        async fn await_the_supervisor_task(self) {
            timeout(A_BOUNDED_WAIT, self.task)
                .await
                .expect("the supervising task never ended")
                .expect("the supervising task panicked");
        }
    }

    #[tokio::test]
    async fn joins_the_common_room_the_configuration_names_when_the_daemon_starts() {
        // Given a daemon configured for one common room
        let mut daemon = a_daemon_supervising(Some(a_livekit_block().with_url(LOBBY_A).build()));

        // When its supervisor runs
        // Then it joins that room
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, THE_LOBBY))
        );
    }

    #[tokio::test]
    async fn replaces_the_running_connection_when_the_livekit_url_changes() {
        // Given a daemon in one common room
        let mut daemon = a_daemon_supervising(Some(a_livekit_block().with_url(LOBBY_A).build()));
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, THE_LOBBY))
        );

        // When it is reconfigured onto another server
        daemon.reconfigure(Some(a_livekit_block().with_url(LOBBY_B).build()));

        // Then it leaves the room it was in before it joins the one it was given
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Left(the_room(LOBBY_A, THE_LOBBY))
        );
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_B, THE_LOBBY))
        );
    }

    #[tokio::test]
    async fn rejoins_the_same_server_under_the_new_name_when_only_the_room_changes() {
        // Given a daemon in one common room
        let mut daemon = a_daemon_supervising(Some(
            a_livekit_block()
                .with_url(LOBBY_A)
                .with_room("the-old-lobby")
                .build(),
        ));
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, "the-old-lobby"))
        );

        // When it is reconfigured onto a different room on the same server
        daemon.reconfigure(Some(
            a_livekit_block()
                .with_url(LOBBY_A)
                .with_room("the-new-lobby")
                .build(),
        ));

        // Then the connection is replaced, not left addressing the room it no longer serves
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Left(the_room(LOBBY_A, "the-old-lobby"))
        );
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, "the-new-lobby"))
        );
    }

    #[rstest]
    #[case::the_livekit_block_is_removed(None)]
    #[case::the_livekit_block_names_no_server(Some(a_livekit_block().with_no_url().build()))]
    #[case::the_livekit_block_names_no_room(Some(a_livekit_block().with_no_room().build()))]
    #[case::the_livekit_block_carries_no_credentials(Some(a_livekit_block().with_no_api_key().build()))]
    #[tokio::test]
    async fn leaves_the_daemon_disconnected_rather_than_on_the_room_it_was_told_to_leave(
        #[case] unjoinable: Option<LiveKitConfig>,
    ) {
        // Given a daemon in one common room
        let mut daemon = a_daemon_supervising(Some(a_livekit_block().with_url(LOBBY_A).build()));
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, THE_LOBBY))
        );

        // When it is reconfigured with something it cannot join
        daemon.reconfigure(unjoinable);

        // Then it leaves the room it was in, and the next thing that happens to it is the join a
        // later, joinable configuration asks for — nothing was left running in between
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Left(the_room(LOBBY_A, THE_LOBBY))
        );
        daemon.reconfigure(Some(a_livekit_block().with_url(LOBBY_C).build()));
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_C, THE_LOBBY))
        );
    }

    #[tokio::test]
    async fn joins_nothing_when_the_daemon_starts_without_a_joinable_common_room() {
        // Given a daemon with no LiveKit block
        let mut daemon = a_daemon_supervising(None);

        // When it is later given one
        daemon.reconfigure(Some(a_livekit_block().with_url(LOBBY_B).build()));

        // Then that is the first room it joins — it was in none before
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_B, THE_LOBBY))
        );
    }

    #[tokio::test]
    async fn leaves_the_common_room_when_the_daemon_drops_its_supervisor() {
        // Given a daemon in a common room
        let mut daemon = a_daemon_supervising(Some(a_livekit_block().with_url(LOBBY_A).build()));
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Joined(the_room(LOBBY_A, THE_LOBBY))
        );

        // When the daemon shuts down
        daemon.drop_the_supervisor();

        // Then it leaves the room, and the task that owned it ends
        assert_eq!(
            daemon.next_room_lifecycle().await,
            RoomLifecycle::Left(the_room(LOBBY_A, THE_LOBBY))
        );
        daemon.await_the_supervisor_task().await;
    }

    #[test]
    fn takes_the_four_connection_strings_from_a_complete_livekit_block() {
        // Given a complete LiveKit block
        let livekit = a_livekit_block()
            .with_url(LOBBY_A)
            .with_room(THE_LOBBY)
            .build();

        // When the room it names is resolved
        let target = CommonRoomTarget::from_livekit(Some(&livekit))
            .expect("a complete livekit block named no room to join");

        // Then it carries what a join needs
        assert_eq!(
            (
                target.url(),
                target.api_key(),
                target.api_secret(),
                target.room()
            ),
            (LOBBY_A, "devkey", "the-secret", THE_LOBBY)
        );
    }

    #[rstest]
    #[case::no_livekit_block(None)]
    #[case::no_url(Some(a_livekit_block().with_no_url().build()))]
    #[case::no_api_key(Some(a_livekit_block().with_no_api_key().build()))]
    #[case::no_api_secret(Some(a_livekit_block().with_no_api_secret().build()))]
    #[case::no_room(Some(a_livekit_block().with_no_room().build()))]
    #[case::a_blank_room(Some(a_livekit_block().with_a_blank_room().build()))]
    fn names_no_room_to_join_when_the_livekit_block_is_incomplete(
        #[case] livekit: Option<LiveKitConfig>,
    ) {
        // When the room an incomplete block names is resolved
        let target = CommonRoomTarget::from_livekit(livekit.as_ref());

        // Then there is none: a partially configured block is not a room
        assert!(
            target.is_none(),
            "an incomplete livekit block was treated as a joinable room"
        );
    }
}
