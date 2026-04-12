//! Regression: common-room discovery must keep the shared LiveKit `room_slot` filled after
//! `set_metadata` succeeds. Calling `set_metadata` before the join handshake finishes used to hit
//! the LiveKit Rust SDK’s 5s signaling timeout and clear the slot in a tight reconnect loop.
//!
//! This test joins another participant first (closer to real `tddy-lobby` load), waits for
//! `room_slot` to populate, then asserts it stays populated across a window where the old bug would
//! have cycled several times.
//!
//! Run (requires Docker LiveKit or `LIVEKIT_TESTKIT_WS_URL`):
//! `cargo test -p tddy-daemon --test common_room_set_metadata_handshake_repro -- --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{Room, RoomOptions};
use serial_test::serial;
use tddy_daemon::config::DaemonConfig;
use tddy_livekit_testkit::LiveKitTestkit;

const COMMON_ROOM: &str = "repro-metadata-handshake-room";
const DAEMON_IDENTITY: &str = "repro-metadata-handshake-daemon";
const PREJOIN_IDENTITY: &str = "repro-prejoin-participant";
const LIVEKIT_API_KEY: &str = "devkey";
const LIVEKIT_API_SECRET: &str = "secret";

fn write_livekit_config(ws_url: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_tools:
  - path: /bin/true
    label: t
daemon_instance_id: {DAEMON_IDENTITY}
livekit:
  url: {ws_url}
  api_key: {LIVEKIT_API_KEY}
  api_secret: {LIVEKIT_API_SECRET}
  common_room: {COMMON_ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

fn spawn_common_room_discovery(
    config: DaemonConfig,
) -> Arc<tokio::sync::RwLock<Option<Arc<Room>>>> {
    let config_arc = Arc::new(config);
    let registry = Arc::new(tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
        config_arc,
        registry,
        room_slot.clone(),
        std::sync::Arc::new(tddy_daemon::tunnel_supervisor::TunnelSupervisor::new()),
    );
    room_slot
}

async fn wait_until_room_slot_populated(
    slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    timeout: Duration,
) {
    tokio::time::timeout(timeout, async {
        loop {
            let filled = { slot.read().await.is_some() };
            if filled {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timeout waiting for common-room room_slot (identity={DAEMON_IDENTITY}) — discovery never published metadata / stored Room handle"
        );
    });
}

#[tokio::test]
#[serial]
async fn common_room_room_slot_stays_populated_after_metadata_publish_with_peer_in_room() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let url = livekit.get_ws_url();
    let pre_token = livekit
        .generate_token(COMMON_ROOM, PREJOIN_IDENTITY)
        .expect("token for pre-join participant");
    let (_pre_room, _pre_ev) = Room::connect(&url, &pre_token, RoomOptions::default())
        .await
        .expect("pre-join participant enters common room");

    let (_cfg_dir, cfg_path) = write_livekit_config(&url);
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let room_slot = spawn_common_room_discovery(config);

    wait_until_room_slot_populated(&room_slot, Duration::from_secs(60)).await;

    tokio::time::sleep(Duration::from_secs(12)).await;

    assert!(
        room_slot.read().await.is_some(),
        "room_slot cleared within 12s after initial fill — expect stable discovery (regression: set_metadata signaling timeout reconnect loop)"
    );
}
