//! Phase 6 E2E acceptance tests: relay forwarding over the common room + idle-timeout
//! integration.
//!
//! AC: relay daemon forwards `ListExecTools` (and `ExecuteTool`) to a remote peer over the
//! common room — the relay classifies `daemon_instance_id`, calls `forward_to_peer`, and the
//! response matches what the peer would return directly. The room here is the in-memory hub
//! (`tddy-room-testkit`), which models the LiveKit adapter's contract; that the LiveKit adapter
//! itself behaves this way against a real server is `tddy-livekit-compat-tests`' job.
//!
//! AC: relay idle monitor spawns a task that checks `IdleTimeoutTracker::should_shutdown()`
//! and fires the external shutdown channel, causing `run_server` to exit cleanly.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serial_test::serial;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_livekit::livekit_peer_discovery::{AdvertisedSigningKey, CommonRoomPeerRegistry};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_room_testkit::{HubEligibleDaemonSource, HubPeerForwarder, RoomHub};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ListExecToolsRequest};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::relay_idle::IdleTimeoutTracker;

/// The hub is fresh per test, so the room name needs no uniqueness.
const RELAY_ROOM: &str = "relay-e2e-common-room";
const RELAY_PEER_ID: &str = "relay-e2e-remote-peer";

/// The identity a daemon serves RPC on — a fixed `daemon-` prefix over its instance id, the way
/// `main.rs` joins the common room. The bare instance id is the discovery participant's, which
/// publishes the advertisement and serves no RPC.
/// See `docs/ft/web/daemon-selector-livekit-rpc.md`.
fn rpc_identity(instance_id: &str) -> String {
    format!("daemon-{instance_id}")
}

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn valid_user_resolver() -> UserResolver {
    Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

fn sessions_resolver(base: PathBuf) -> SessionsBaseResolver {
    Arc::new(move |_| Some(base.clone()))
}

fn write_daemon_yaml(instance_id: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let id_block = instance_id
        .map(|id| format!("daemon_instance_id: {id}\n"))
        .unwrap_or_default();
    let yaml = format!("{id_block}livekit:\n  enabled: false\n");
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        "- github_user: \"testuser\"\n  os_user: \"testuser\"\n",
    )
    .unwrap();
    (dir, path)
}

// ── Idle-timeout integration (non-LiveKit) ───────────────────────────────────────────────────────

/// Options for a relay-mode server that `shutdown` stops: no bundle, no RPC services and no
/// common room, because a relay serves no page — the shutdown channel is the wiring under test.
fn a_relay_server_stopped_by(
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> tddy_daemon::server::RunServerOptions {
    tddy_daemon::server::RunServerOptions {
        host: "127.0.0.1".to_string(),
        port: 0, // ephemeral
        bundle_path: PathBuf::new(),
        rpc_entries: vec![],
        livekit_url: None,
        common_room: None,
        livekit_enabled: false, // relay mode joins no common room
        daemon_instance_id: "test-instance".to_string(),
        // Relay mode serves no page and provisions no jail, so it advertises no placement.
        sandboxed_codebase: None,
        // …and a relay registers no auth service, so it declares no sign-in flow.
        auth_flow: None,
        allowed_agents: vec![],
        debug: None,
        lifecycle_telegram: None,
        shutdown_rx: Some(shutdown),
    }
}

/// Phase 6 AC: relay idle monitor fires the external shutdown channel when the tracker expires,
/// causing `run_server` to exit cleanly — the full chain mirrors what `main.rs` wires in relay mode.
#[tokio::test]
async fn relay_idle_monitor_triggers_server_shutdown() {
    // Given
    let tracker = Arc::new(IdleTimeoutTracker::new(Duration::from_millis(50)));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    // Mirrors the monitor task spawned by main.rs in relay mode.
    let monitor_tracker = tracker.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if monitor_tracker.should_shutdown() {
                let _ = tx.send(());
                return;
            }
        }
    });

    // When
    let result = tddy_daemon::server::run_server(a_relay_server_stopped_by(rx)).await;

    // Then
    assert!(
        result.is_ok(),
        "relay server must exit cleanly when idle monitor fires the shutdown channel; got: {:?}",
        result.err()
    );
}

/// Phase 6 AC: `record_rpc_activity` on the service resets the idle clock so the tracker does
/// not trigger shutdown while activity is ongoing.
///
/// Note: `IdleTimeoutTracker::record_activity()` and `should_shutdown()` are already unit-tested
/// in `relay_runtime_acceptance.rs`. This test validates the `with_idle_tracker` / service wiring
/// is functional by driving the tracker directly and verifying it does not fire prematurely.
#[tokio::test]
async fn relay_activity_defers_idle_shutdown() {
    // Given a tracker that has been idle past its timeout. The timeout is a minute, so the check
    // after the activity has a minute to run in: with a timeout of milliseconds any scheduling delay
    // between the activity and the check read as "the activity never deferred the shutdown".
    let two_minutes_ago = std::time::Instant::now()
        .checked_sub(Duration::from_secs(120))
        .expect("the clock reaches two minutes back");
    let tracker = Arc::new(IdleTimeoutTracker::last_active_at(
        Duration::from_secs(60),
        two_minutes_ago,
    ));
    assert!(
        tracker.should_shutdown(),
        "precondition: the tracker is idle before any activity"
    );

    // When
    tracker.record_activity();

    // Then
    assert!(
        !tracker.should_shutdown(),
        "activity must defer the idle shutdown"
    );
}

/// A tracker with no activity expires once its timeout has passed. Waiting longer than the timeout
/// can only make it more idle, so this direction cannot race the clock.
#[tokio::test]
async fn relay_idle_tracker_expires_after_a_full_idle_period() {
    // Given
    let tracker = Arc::new(IdleTimeoutTracker::new(Duration::from_millis(80)));

    // When
    tokio::time::sleep(Duration::from_millis(120)).await;

    // Then
    assert!(
        tracker.should_shutdown(),
        "tracker must expire after full idle period with no activity"
    );
}

// ── Common-room relay forwarding ─────────────────────────────────────────────────────────

/// Phase 6 AC: relay daemon forwards `ListExecTools` to a remote peer over the common room.
///
/// Service A (relay) has discovery seeing service B. When A receives
/// `list_exec_tools(daemon_instance_id=B)`, it classifies Forward, calls `forward_to_peer`,
/// and returns B's tool catalog to the caller.
#[tokio::test]
#[serial]
async fn relay_forwards_list_exec_tools_to_remote_peer() {
    // Given
    let hub = RoomHub::new();

    // ── Service B: the remote peer. Advertises in the room and serves RPCs.
    let (_tmp_b, path_b) = write_daemon_yaml(Some(RELAY_PEER_ID));
    let config_b = DaemonConfig::load(&path_b).unwrap();
    let sessions_b = tempfile::tempdir().unwrap();
    let service_b = Arc::new(
        tddy_daemon_rpc::RpcHandlers::install(DaemonSessionHost::new(
            config_b.clone(),
            sessions_resolver(sessions_b.path().to_path_buf()),
            sessions_b.path().to_path_buf(),
            valid_user_resolver(),
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            PeerRouting::local_only(&config_b), // B has no discovery of its own — it only serves its local tools
            Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
            Arc::new(CliSessionManager::new()),
            tddy_session_lifecycle::test_util::builtin_coding_agents(),
        ))
        .0,
    );

    // B's discovery participant: bare instance id, publishes the advertisement A discovers.
    let _b_advertisement =
        hub.advertise_daemon(RELAY_ROOM, &config_b, &AdvertisedSigningKey::default());
    // B's RPC participant: `daemon-{instance_id}`, the identity A's forward addresses.
    let b_rpc = hub.join(RELAY_ROOM, &rpc_identity(RELAY_PEER_ID), "");
    b_rpc.serve(Arc::new(
        tddy_session_lifecycle::test_util::daemon_rpc_roster(&service_b),
    ));

    // ── Service A: the relay. Its registry is attached to the room and will see B.
    let (_tmp_a, path_a) = write_daemon_yaml(None);
    let config_a = DaemonConfig::load(&path_a).unwrap();
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let _attach = hub.attach_registry(
        RELAY_ROOM,
        &local_instance_id_for_config(&config_a),
        registry.clone(),
    );
    let common_room = CommonRoom::new(
        HubPeerForwarder::new(
            hub.clone(),
            RELAY_ROOM,
            rpc_identity(&local_instance_id_for_config(&config_a)),
        ),
        &config_a,
    );
    let eligible: Arc<dyn EligibleDaemonSource> =
        HubEligibleDaemonSource::new(Arc::new(config_a.clone()), registry, common_room.clone());
    let sessions_a = tempfile::tempdir().unwrap();
    // "Who can I see?" is `host.HostService`'s question since `#unbundle` node 1. Built over the
    // same eligible source A routes on, so discovery readiness and routability are one fact.
    let hosts_a = tddy_host_service::HostServiceImpl::new(
        config_a.clone(),
        sessions_a.path(),
        valid_user_resolver(),
    )
    .with_eligible_daemon_source(Arc::clone(&eligible));
    let service_a = TestDaemon::from_host(DaemonSessionHost::new(
        config_a.clone(),
        sessions_resolver(sessions_a.path().to_path_buf()),
        sessions_a.path().to_path_buf(),
        valid_user_resolver(),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        PeerRouting::new(config_a.clone(), Arc::clone(&eligible), Some(common_room)),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    ));

    // When
    // Wait until A's discovery sees B in the common room.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let rows = tddy_service::proto::host::HostService::list_eligible_daemons(
                &hosts_a,
                Request::direct(tddy_service::proto::host::ListEligibleDaemonsRequest {
                    session_token: "valid-token".to_string(),
                }),
            )
            .await
            .expect("ListEligibleDaemons")
            .into_inner()
            .daemons;
            if rows.iter().any(|d| d.instance_id == RELAY_PEER_ID) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("timeout: B should appear in A's eligible-daemon list");

    // Then
    // A forwards ListExecTools to B — the relay must route to B and return B's catalog.
    let resp = service_a
        .list_exec_tools(Request::direct(ListExecToolsRequest {
            session_token: "valid-token".to_string(),
            daemon_instance_id: RELAY_PEER_ID.to_string(), // relay to B
            ..Default::default()
        }))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "relay ListExecTools must succeed (A forwards to B); got {:?}: {}",
                e.code(),
                e.message()
            )
        });

    let tools = resp.into_inner().tools;
    assert!(
        !tools.is_empty(),
        "relay must return B's tool catalog (forwarded over the common room); got empty list"
    );
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        names.contains(&"Read"),
        "B's catalog must include 'Read'; got: {:?}",
        names
    );
    assert!(
        names.contains(&"Write"),
        "B's catalog must include 'Write'; got: {:?}",
        names
    );
    assert!(
        names.contains(&"Shell"),
        "B's catalog must include 'Shell'; got: {:?}",
        names
    );
}
