//! The common-room discovery loop reports its progress on the integration status board: a daemon
//! joining its room is `Connected`, one whose server does not answer is `Unreachable`.
//!
//! Run (requires Docker LiveKit or `LIVEKIT_TESTKIT_WS_URL`):
//! `cargo test -p tddy-daemon-livekit --test common_room_integration_status`

use std::sync::Arc;
use std::time::Duration;

use serial_test::serial;
use tddy_daemon_extensions::{IntegrationState, IntegrationStatusBoard};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::{
    spawn_common_room_discovery_loop, CommonRoomPeerRegistry,
};
use tddy_daemon_livekit::LIVEKIT_INTEGRATION_ID;
use tddy_livekit_testkit::LiveKitTestkit;

const LIVEKIT_API_KEY: &str = "devkey";
const LIVEKIT_API_SECRET: &str = "secret";

/// A daemon config whose common room is `room` on the LiveKit server at `ws_url`.
fn a_daemon_config_joining(ws_url: &str, room: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_tools:
  - path: /bin/true
    label: t
daemon_instance_id: status-board-daemon
livekit:
  enabled: true
  url: {ws_url}
  api_key: {LIVEKIT_API_KEY}
  api_secret: {LIVEKIT_API_SECRET}
  common_room: {room}
"#
    );
    serde_yaml::from_str(&yaml).expect("daemon yaml")
}

/// A LiveKit address nothing listens on: bound, then released, so a dial is refused at once.
async fn a_livekit_address_nothing_listens_on() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binding a loopback port must succeed");
    let addr = listener
        .local_addr()
        .expect("a bound listener must know its address");
    drop(listener);
    format!("ws://{addr}")
}

/// The discovery loop running for `config`, reporting on the returned board.
fn the_discovery_loop_for(
    config: DaemonConfig,
) -> (Arc<IntegrationStatusBoard>, tokio::task::JoinHandle<()>) {
    let board = IntegrationStatusBoard::new();
    let handle = spawn_common_room_discovery_loop(
        Arc::new(config),
        Arc::new(CommonRoomPeerRegistry::new()),
        Arc::new(tokio::sync::RwLock::new(None)),
        Default::default(),
        board.reporter(LIVEKIT_INTEGRATION_ID, "LiveKit"),
    );
    (board, handle)
}

fn livekit_on(board: &IntegrationStatusBoard) -> IntegrationState {
    board
        .snapshot()
        .into_iter()
        .find(|integration| integration.id == LIVEKIT_INTEGRATION_ID)
        .expect("the loop's reporter is registered on the board")
        .state
}

/// Poll until LiveKit on `board` satisfies `reached`; panic with the last state on timeout.
async fn wait_until_livekit_is(
    board: &IntegrationStatusBoard,
    what: &str,
    reached: impl Fn(&IntegrationState) -> bool,
) {
    let outcome = tokio::time::timeout(Duration::from_secs(60), async {
        while !reached(&livekit_on(board)) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    assert!(
        outcome.is_ok(),
        "timeout waiting for LiveKit to be {what}; last state {:?}",
        livekit_on(board)
    );
}

#[tokio::test]
#[serial(livekit_docker)]
async fn joining_the_common_room_reports_livekit_connected() {
    // Given a LiveKit server and a daemon configured to join a room on it
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let room = LiveKitTestkit::unique_room("status-board-connected");
    let config = a_daemon_config_joining(&livekit.get_ws_url(), &room);

    // When the discovery loop runs
    let (board, discovery) = the_discovery_loop_for(config);

    // Then the board reports LiveKit connected
    wait_until_livekit_is(&board, "connected", |s| *s == IntegrationState::Connected).await;
    discovery.abort();
}

#[tokio::test]
async fn a_server_that_does_not_answer_reports_livekit_unreachable() {
    // Given a daemon configured to join a room on a server nothing listens on
    let config = a_daemon_config_joining(
        &a_livekit_address_nothing_listens_on().await,
        "status-board-unreachable",
    );

    // When the discovery loop runs
    let (board, discovery) = the_discovery_loop_for(config);

    // Then the board reports LiveKit unreachable
    wait_until_livekit_is(&board, "unreachable", |s| {
        matches!(s, IntegrationState::Unreachable { .. })
    })
    .await;
    discovery.abort();
}
