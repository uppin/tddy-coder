//! Acceptance tests: idle-timeout tracker wired into DaemonSessionHost (Phase 3 follow-up).
//!
//! AC: in relay mode, every RPC call bumps the `IdleTimeoutTracker`'s last-activity timestamp so
//! the daemon does not self-terminate while a remote session is active.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListToolsRequest};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::relay_idle::IdleTimeoutTracker;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn minimal_service_with_tracker(tracker: Arc<IdleTimeoutTracker>) -> TestDaemon {
    let sessions_base: SessionsBaseResolver =
        Arc::new(|_| Some(std::env::temp_dir().join("test-sessions")));
    let user_resolver: UserResolver = Arc::new(|_| None);

    TestDaemon::from_host(
        DaemonSessionHost::new(
            DaemonConfig::default(),
            sessions_base,
            std::env::temp_dir().join("tddy-idle-test"),
            user_resolver,
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&DaemonConfig::default()),
            Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
            Arc::new(CliSessionManager::new()),
            tddy_agent_launch::host_coding_agents::no_coding_agents(),
        )
        .with_idle_tracker(tracker),
    )
}

/// Phase 3 AC: after calling any RPC on a service with an injected idle tracker,
/// `should_shutdown()` returns false — the activity timestamp was bumped.
#[tokio::test]
async fn rpc_call_bumps_idle_tracker_so_shutdown_is_not_triggered() {
    // Given
    // Let the timeout expire before the RPC call. It is long enough that the check after the call
    // cannot itself outlive it: with a 1 ms timeout, any scheduling delay between the RPC's bump and
    // `should_shutdown` read as "the RPC never bumped it".
    let timeout = Duration::from_millis(500);
    let tracker = Arc::new(IdleTimeoutTracker::new(timeout));
    std::thread::sleep(timeout + Duration::from_millis(100));

    assert!(
        tracker.should_shutdown(),
        "precondition: tracker must be expired before the RPC call"
    );

    let service = minimal_service_with_tracker(Arc::clone(&tracker));

    // When
    // Make any RPC — ListTools is the simplest (no auth required).
    let _ = service
        .list_tools(Request::direct(ListToolsRequest {}))
        .await;

    // Then
    assert!(
        !tracker.should_shutdown(),
        "idle tracker must be reset after an RPC call — should_shutdown must be false"
    );
}

/// Phase 3 AC: `DaemonSessionHost::with_idle_tracker` is a builder that attaches
/// a tracker without changing the main `new()` signature (existing callers pass no tracker).
#[tokio::test]
async fn connection_service_accepts_idle_tracker_via_builder() {
    // Given
    let tracker = Arc::new(IdleTimeoutTracker::new(Duration::from_secs(300)));

    // When
    // This is purely a compile-time check — if `with_idle_tracker` doesn't exist, the test fails.
    let _service = minimal_service_with_tracker(Arc::clone(&tracker));

    // Then
    assert!(
        !tracker.should_shutdown(),
        "fresh tracker must not indicate shutdown"
    );
}
