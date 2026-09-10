//! LiveKit `common_room` peer discovery — acceptance tests from the feature PRD Testing Plan.
//!
//! Spins up [`tddy_livekit_testkit::LiveKitTestkit`] (Docker container unless `LIVEKIT_TESTKIT_WS_URL`
//! points at a running server). Uses the production `HostServiceImpl` with
//! `LiveKitEligibleDaemonSource`, `spawn_common_room_discovery_loop`,
//! and the shared room slot (same wiring as `main` when `livekit.common_room` is configured).
//!
//! Stays in `tddy-daemon` rather than moving to `tddy-host-service` with the RPC it drives: the
//! wiring under test is the daemon's own common-room discovery, and `tddy-host-service` cannot
//! depend on the crate that owns it. What did change is *what it calls* — the service, not the
//! connection service.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{Room, RoomOptions};
use serial_test::serial;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::HostServiceImpl;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::host::{EligibleDaemonEntry, HostService, ListEligibleDaemonsRequest};

const COMMON_ROOM: &str = "acceptance-common-room";
const PEER_INSTANCE_ID: &str = "acceptance-daemon-b";
const LIVEKIT_API_KEY: &str = "devkey";
const LIVEKIT_API_SECRET: &str = "secret";

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
  enabled: true
  url: {ws_url}
  api_key: {LIVEKIT_API_KEY}
  api_secret: {LIVEKIT_API_SECRET}
  common_room: {COMMON_ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

fn host_service_with_livekit_discovery(
    config: DaemonConfig,
) -> (HostServiceImpl, tempfile::TempDir) {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let config_arc = Arc::new(config.clone());
    let registry =
        Arc::new(tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    // The discovery loop alone. It used to be reached through `spawn_common_room_discovery_task`,
    // which also started the OAuth loopback tunnel supervisor — a co-tenant of the room slot with
    // nothing to do with peer discovery, and one that now lives in `tddy-daemon-auth`, which this
    // crate deliberately cannot reach. The supervisor acts only on participants publishing pending
    // `codex_oauth` metadata, and nothing here publishes any, so what this suite observes is
    // unchanged.
    tddy_daemon_livekit::livekit_peer_discovery::spawn_common_room_discovery_loop(
        config_arc.clone(),
        registry.clone(),
        room_slot.clone(),
    );
    let eligible: Arc<dyn tddy_host_service::multi_host::EligibleDaemonSource> = Arc::new(
        tddy_daemon_livekit::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(
            config_arc,
            registry,
            room_slot.clone(),
        ),
    );
    let service = HostServiceImpl::new(config, sessions_tmp.path(), user_resolver)
        .with_eligible_daemon_source(eligible)
        .with_common_room(room_slot);
    (service, sessions_tmp)
}

/// Wait until discovery sync sees the peer (bounded; avoids flake from fixed sleeps).
async fn wait_until_peer_listed(service: &HostServiceImpl, instance_id: &str) {
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

async fn list_eligible(svc: &HostServiceImpl) -> Vec<EligibleDaemonEntry> {
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
    // A real `tddy-daemon` peer publishes a daemon advertisement (`set_metadata`); eligibility has
    // no identity fallback, so without it the local daemon never lists this peer. Best-effort:
    // `set_metadata` can report a timeout in livekit 0.7.x even after the value propagates.
    let advertisement = format!(
        r#"{{"instance_id":"{PEER_INSTANCE_ID}","label":"{PEER_INSTANCE_ID} peer daemon"}}"#
    );
    let _ = tokio::time::timeout(
        Duration::from_secs(10),
        room.local_participant().set_metadata(advertisement),
    )
    .await;
    room
}

/// When another daemon shares `livekit.common_room`, `ListEligibleDaemons` must include that peer
/// (`instance_id` matches the peer’s configured id) with `is_local: false`.
#[tokio::test]
#[serial]
async fn list_eligible_daemons_includes_discovered_peer_when_second_daemon_in_common_room() {
    // Given
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = host_service_with_livekit_discovery(config);
    let _peer = join_second_daemon_participant(&livekit).await;
    wait_until_peer_listed(&service, PEER_INSTANCE_ID).await;

    // When
    let daemons = list_eligible(&service).await;

    // Then
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
    // Given
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = host_service_with_livekit_discovery(config);
    let _peer = join_second_daemon_participant(&livekit).await;
    wait_until_peer_listed(&service, PEER_INSTANCE_ID).await;

    // When
    let daemons = list_eligible(&service).await;

    // Then
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
    // Given
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let (_cfg_dir, cfg_path) = write_livekit_config(&livekit.get_ws_url());
    let config = DaemonConfig::load(&cfg_path).expect("daemon yaml");
    let (service, _sessions_tmp) = host_service_with_livekit_discovery(config);
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

    // When
    let _ = peer_room.close().await;

    // Then
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
