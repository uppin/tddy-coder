//! Reproduction: a daemon's common-room RPC participant (`daemon-{instance}`) that loses its room
//! for good — the LiveKit SDK gives up reconnecting, or the server removes it — never rejoins.
//!
//! `LiveKitParticipant::run_with_reconnect` joins once and returns when the connection ends, and
//! `CommonRoomSupervisorTask` only rejoins when the configured room changes. The discovery
//! participant keeps reconnecting and advertising the daemon, so remote web clients still list the
//! host while nothing in the room answers their RPCs. Observed on Tddy Desktop: the RPC
//! participant dropped at 17:17:56 and stayed gone for 12h40m until the app was restarted.
//!
//! Run (requires Docker LiveKit or `LIVEKIT_TESTKIT_WS_URL`):
//! `cargo test -p tddy-daemon-livekit --test common_room_rpc_participant_rejoin_repro -- --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serial_test::serial;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::common_room_supervisor::DaemonCommonRoomConnector;
use tddy_daemon_livekit::SupervisedCommonRoom;
use tddy_livekit_testkit::LiveKitTestkit;

const THE_DAEMON_INSTANCE: &str = "repro-desktop-host";
const ITS_RPC_PARTICIPANT: &str = "daemon-repro-desktop-host";

/// Long enough for a first join against a cold container; a bound, not a duration.
const A_FIRST_JOIN: Duration = Duration::from_secs(60);
/// The longest a remote client may find the daemon's RPC participant missing after it was lost.
/// Generous next to a join, which takes well under a second against the testkit server.
const A_BOUNDED_REJOIN: Duration = Duration::from_secs(30);

/// A LiveKit server with this daemon's common room on it.
struct TheLobby {
    livekit: LiveKitTestkit,
    room: String,
}

impl TheLobby {
    async fn on_a_livekit_server() -> Self {
        Self {
            livekit: LiveKitTestkit::start()
                .await
                .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)"),
            room: LiveKitTestkit::unique_room("repro-rpc-participant-rejoin"),
        }
    }

    /// The configuration Tddy Desktop runs with: a LiveKit block naming this lobby, and a fixed
    /// instance id so the RPC participant's identity is known in advance.
    fn a_daemon_configured_for_it(&self) -> (tempfile::TempDir, DaemonConfig) {
        let dir = tempfile::tempdir().expect("config dir");
        let path: PathBuf = dir.path().join("desktop.yaml");
        let yaml = format!(
            r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_tools:
  - path: /bin/true
    label: t
daemon_instance_id: {THE_DAEMON_INSTANCE}
livekit:
  enabled: true
  url: {url}
  api_key: devkey
  api_secret: secret
  common_room: {room}
"#,
            url = self.livekit.get_ws_url(),
            room = self.room,
        );
        std::fs::write(&path, yaml).expect("write desktop.yaml");
        let config = DaemonConfig::load(&path).expect("desktop.yaml parses");
        (dir, config)
    }

    /// The server dropping `identity` for good — what the daemon sees once the SDK gives up
    /// reconnecting after a network outage.
    async fn loses(&self, identity: &str) {
        self.livekit
            .remove_participant(&self.room, identity)
            .await
            .expect("the server evicts the participant");
    }

    /// Whether the server holds `identity` as connected in the lobby — joined with ICE up, which a
    /// participant still negotiating its first connection is not.
    async fn has_connected(&self, identity: &str) -> bool {
        self.livekit
            .active_participant_identities(&self.room)
            .await
            .expect("the server lists the lobby's participants")
            .iter()
            .any(|active| active == identity)
    }

    /// Waits until `identity` is connected in the lobby, or panics saying it never was.
    async fn eventually_has_connected(&self, identity: &str, within: Duration) {
        tokio::time::timeout(within, async {
            while !self.has_connected(identity).await {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!("{identity} was not connected in the lobby {within:?} later — remote clients cannot reach this daemon")
        });
    }

    /// Waits until `identity` is no longer connected, so a later "it is back" is a rejoin rather
    /// than the original participant still being there.
    async fn eventually_has_lost(&self, identity: &str, within: Duration) {
        tokio::time::timeout(within, async {
            while self.has_connected(identity).await {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!("{identity} was still connected {within:?} after the server evicted it")
        });
    }
}

/// A daemon supervising its common room the way `tddy-daemon`'s runtime wires it: the real
/// connector, with no peer discovery, so only the RPC participant is in play.
fn a_daemon_supervising_its_common_room(config: DaemonConfig) -> SupervisedCommonRoom {
    let supervisor = SupervisedCommonRoom::new(config.livekit.as_ref());
    let connector = Arc::new(DaemonCommonRoomConnector::new(config, Vec::new(), None));
    tokio::spawn(supervisor.task(connector).run());
    supervisor
}

#[tokio::test]
#[serial(livekit_docker)]
async fn rejoins_the_common_room_after_the_rpc_participant_loses_it() {
    // Given a daemon whose RPC participant is serving in its common room
    let lobby = TheLobby::on_a_livekit_server().await;
    let (_config_dir, config) = lobby.a_daemon_configured_for_it();
    let _daemon = a_daemon_supervising_its_common_room(config);
    lobby
        .eventually_has_connected(ITS_RPC_PARTICIPANT, A_FIRST_JOIN)
        .await;

    // When the connection behind that participant is lost for good
    lobby.loses(ITS_RPC_PARTICIPANT).await;
    lobby
        .eventually_has_lost(ITS_RPC_PARTICIPANT, A_BOUNDED_REJOIN)
        .await;

    // Then the daemon puts it back, with no configuration change and no restart
    lobby
        .eventually_has_connected(ITS_RPC_PARTICIPANT, A_BOUNDED_REJOIN)
        .await;
}
