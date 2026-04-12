//! Reproduction: after a second LiveKit client joins `livekit.common_room` with the same
//! participant identity as the daemon, the daemon loses the room. Once that client disconnects,
//! common-room discovery must repopulate the shared `room_slot` within a bounded time so RPC and
//! presence recover.
//!
//! Run (requires Docker LiveKit or `LIVEKIT_TESTKIT_WS_URL`):
//! `cargo test -p tddy-daemon --test common_room_duplicate_identity_repro -- --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{Room, RoomOptions};
use serial_test::serial;
use tddy_daemon::config::DaemonConfig;
use tddy_livekit_testkit::LiveKitTestkit;

const COMMON_ROOM: &str = "repro-dup-common-room";
const SHARED_IDENTITY: &str = "repro-shared-daemon-identity";
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
daemon_instance_id: {SHARED_IDENTITY}
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
            "timeout waiting for common-room room_slot to hold a LiveKit Room (identity={SHARED_IDENTITY})"
        );
    });
}

async fn wait_until_room_slot_cleared(
    slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    timeout: Duration,
) {
    tokio::time::timeout(timeout, async {
        loop {
            let empty = { slot.read().await.is_none() };
            if empty {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timeout waiting for room_slot to clear after DuplicateIdentity (identity={SHARED_IDENTITY})"
        );
    });
}

#[tokio::test]
#[serial]
async fn common_room_room_slot_recovers_after_duplicate_identity_client_leaves() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let room_slot = spawn_common_room_discovery(config);

    wait_until_room_slot_populated(&room_slot, Duration::from_secs(60)).await;

    let url = livekit.get_ws_url();
    let token = livekit
        .generate_token(COMMON_ROOM, SHARED_IDENTITY)
        .expect("token for duplicate participant");
    let (dup_room, _dup_ev) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .expect("second client joins common room with same identity as daemon");

    wait_until_room_slot_cleared(&room_slot, Duration::from_secs(45)).await;

    let _ = dup_room.close().await;

    wait_until_room_slot_populated(&room_slot, Duration::from_secs(90)).await;
}
