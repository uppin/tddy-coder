//! Acceptance tests: idle-timeout tracker wired into DaemonSessionHost (Phase 3 follow-up).
//!
//! AC: in relay mode, every RPC call bumps the `IdleTimeoutTracker`'s last-activity timestamp so
//! the daemon does not self-terminate while a remote session is active.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::DaemonSessionHost;
use tddy_daemon::relay_idle::IdleTimeoutTracker;
use tddy_daemon::test_util::TestDaemon;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListToolsRequest};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn minimal_service_with_tracker(tracker: Arc<IdleTimeoutTracker>) -> TestDaemon {
    let sessions_base: SessionsBaseResolver =
        Arc::new(|_| Some(std::env::temp_dir().join("test-sessions")));
    let user_resolver: UserResolver = Arc::new(|_| None);

    TestDaemon::from_arc(Arc::new(
        DaemonSessionHost::new(
            DaemonConfig::default(),
            sessions_base,
            std::env::temp_dir().join("tddy-idle-test"),
            user_resolver,
            None,
            None,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
        .with_idle_tracker(tracker),
    ))
}

/// Phase 3 AC: after calling any RPC on a service with an injected idle tracker,
/// `should_shutdown()` returns false — the activity timestamp was bumped.
#[tokio::test]
async fn rpc_call_bumps_idle_tracker_so_shutdown_is_not_triggered() {
    // Given
    // Use a 1ms timeout and let it expire before the RPC call.
    let tracker = Arc::new(IdleTimeoutTracker::new(Duration::from_millis(1)));
    std::thread::sleep(Duration::from_millis(10));

    assert!(
        tracker.should_shutdown(),
        "precondition: tracker must be expired before the RPC call"
    );

    let service = minimal_service_with_tracker(Arc::clone(&tracker));

    // When
    // Make any RPC — ListTools is the simplest (no auth required).
    let _ = service.list_tools(Request::new(ListToolsRequest {})).await;

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
