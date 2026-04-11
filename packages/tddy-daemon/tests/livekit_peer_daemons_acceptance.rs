//! LiveKit `common_room` peer discovery — acceptance tests from the feature PRD Testing Plan.
//!
//! Spins up [`tddy_livekit_testkit::LiveKitTestkit`] (Docker container unless `LIVEKIT_TESTKIT_WS_URL`
//! points at a running server). Uses production [`ConnectionServiceImpl`] with
//! `LiveKitEligibleDaemonSource`, `spawn_common_room_discovery_task`,
//! and the shared room slot (same wiring as `main` when `livekit.common_room` is configured).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{Room, RoomOptions};
use serial_test::serial;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListEligibleDaemonsRequest,
};

const COMMON_ROOM: &str = "acceptance-common-room";
const PEER_INSTANCE_ID: &str = "acceptance-daemon-b";
const LIVEKIT_API_KEY: &str = "devkey";
const LIVEKIT_API_SECRET: &str = "secret";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

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

fn connection_service_with_livekit_discovery(
    config: DaemonConfig,
) -> (ConnectionServiceImpl, tempfile::TempDir) {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let sessions_base = sessions_tmp.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let config_arc = Arc::new(config.clone());
    let registry = Arc::new(tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
        config_arc.clone(),
        registry.clone(),
        room_slot.clone(),
    );
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        tddy_daemon::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(config_arc, registry),
    );
    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        Some(
            tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles {
                eligible_daemon_source: eligible,
                common_room_livekit_room: room_slot,
            },
        ),
        None,
    );
    (service, sessions_tmp)
}

/// Wait until discovery sync sees the peer (bounded; avoids flake from fixed sleeps).
async fn wait_until_peer_listed(service: &ConnectionServiceImpl, instance_id: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let daemons = list_eligible(service).await;
            if daemons.iter().any(|d| d.instance_id == instance_id) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("timeout: peer {instance_id} should appear in ListEligibleDaemons (discovery sync)");
    });
}

async fn list_eligible(
    svc: &ConnectionServiceImpl,
) -> Vec<tddy_service::proto::connection::EligibleDaemonEntry> {
    let request = Request::new(ListEligibleDaemonsRequest {
        session_token: "valid-token".to_string(),
    });
    svc.list_eligible_daemons(request)
        .await
        .expect("ListEligibleDaemons RPC")
        .into_inner()
        .daemons
}

async fn join_second_daemon_participant(livekit: &LiveKitTestkit) -> Room {
    let url = livekit.get_ws_url();
    let token = livekit
        .generate_token(COMMON_ROOM, PEER_INSTANCE_ID)
        .expect("generate LiveKit token for peer daemon");
    let (room, _events) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .expect("peer daemon joins common_room");
    room
}

/// When another daemon shares `livekit.common_room`, `ListEligibleDaemons` must include that peer
/// (`instance_id` matches the peer’s configured id) with `is_local: false`.
#[tokio::test]
#[serial]
async fn list_eligible_daemons_includes_discovered_peer_when_second_daemon_in_common_room() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = connection_service_with_livekit_discovery(config);

    let _peer = join_second_daemon_participant(&livekit).await;
    wait_until_peer_listed(&service, PEER_INSTANCE_ID).await;

    let daemons = list_eligible(&service).await;
    let peer = daemons.iter().find(|d| d.instance_id == PEER_INSTANCE_ID);
    assert!(
        peer.is_some(),
        "expected peer {PEER_INSTANCE_ID} in eligible list after second daemon joined common_room {:?}; got {:?}",
        COMMON_ROOM,
        daemons
            .iter()
            .map(|d| (d.instance_id.clone(), d.is_local))
            .collect::<Vec<_>>()
    );
    let peer = peer.unwrap();
    assert!(
        !peer.is_local,
        "discovered peer row must have is_local=false"
    );
    assert!(
        !peer.label.trim().is_empty(),
        "discovered peer must have non-empty label"
    );
}

/// In a multi-daemon common room, exactly one row is `is_local: true` (this process).
#[tokio::test]
#[serial]
async fn list_eligible_daemons_local_exactly_one_is_local() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = connection_service_with_livekit_discovery(config);

    let _peer = join_second_daemon_participant(&livekit).await;
    wait_until_peer_listed(&service, PEER_INSTANCE_ID).await;

    let daemons = list_eligible(&service).await;
    let n_local = daemons.iter().filter(|d| d.is_local).count();
    assert_eq!(
        n_local,
        1,
        "exactly one is_local row; got {:?}",
        daemons
            .iter()
            .map(|d| (d.instance_id.clone(), d.is_local))
            .collect::<Vec<_>>()
    );
    assert!(
        daemons.len() >= 2,
        "with common_room and a second daemon present, list must include local + peer(s); got {} row(s)",
        daemons.len()
    );
    assert!(
        daemons.first().map(|d| d.is_local).unwrap_or(false),
        "ordering policy: local daemon row must be first"
    );
}

/// After LiveKit signals the peer left, the remote row disappears within a bounded window.
#[tokio::test]
#[serial]
async fn peer_list_removes_entry_after_simulated_disconnect() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = connection_service_with_livekit_discovery(config);

    let peer_room = join_second_daemon_participant(&livekit).await;

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let daemons = list_eligible(&service).await;
            if daemons.iter().any(|d| d.instance_id == PEER_INSTANCE_ID) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("peer should appear in ListEligibleDaemons while connected");

    let _ = peer_room.close().await;

    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let daemons = list_eligible(&service).await;
            if !daemons.iter().any(|d| d.instance_id == PEER_INSTANCE_ID) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("peer row should be removed shortly after LiveKit disconnect");
}
