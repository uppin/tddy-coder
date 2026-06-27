//! Acceptance tests: single-screen terminal control mutex.
//!
//! PRD: docs/ft/daemon/terminal-sessions.md (control section).
//!
//! These tests verify that:
//!   1. `ClaudeCliSessionManager` correctly manages control leases (manager-level tests).
//!   2. The `ConnectionService` RPC handlers enforce the lease (RPC-level tests).
//!
//! All tests are green (production implementation in place).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::claude_cli_session::{ClaimOutcome, ClaudeCliSessionManager};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ClaimTerminalControlRequest, ConnectionService as ConnectionServiceTrait, SessionTerminalInput,
    WatchTerminalControlRequest,
};

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "ctrl-test-session";
const SCREEN_A: &str = "screen-a-identity";
const SCREEN_B: &str = "screen-b-identity";
/// Stub binary — /bin/cat stays alive reading PTY stdin for input tests.
const MAIN_STUB: &str = "/bin/cat";

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "users:\n  - github_user: \"testuser\"\n    os_user: \"testuser\"\n";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn make_service(
    manager: Arc<ClaudeCliSessionManager>,
) -> (ConnectionServiceImpl, tempfile::TempDir, tempfile::TempDir) {
    let (cfg_dir, config) = test_config();
    let sessions = tempfile::tempdir().unwrap();
    // `tddy_data_dir` is the daemon's resolved tddy home (config-only source of truth). The test
    // reuses the sessions tempdir path; the TempDir stays alive via the `sessions_base` closure.
    let tddy_data_dir = sessions.path().to_path_buf();
    let sessions_base: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions.path().to_path_buf()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let service = ConnectionServiceImpl::new(
        config,
        sessions_base,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    );
    let sessions = tempfile::tempdir().unwrap();
    (service, cfg_dir, sessions)
}

async fn start_main_terminal(manager: &ClaudeCliSessionManager) -> tempfile::TempDir {
    let worktree = tempfile::tempdir().unwrap();
    manager
        .start(
            SESSION_ID,
            worktree.path().to_path_buf(),
            "claude-opus-4-8",
            MAIN_STUB,
            None,
            None,
        )
        .await
        .expect("main claude terminal must start");
    worktree
}

// ---------------------------------------------------------------------------
// Manager-level: ClaudeCliSessionManager control lease semantics.
// ---------------------------------------------------------------------------

/// **claim_grants_token_when_unheld**: the first screen to claim an uncontrolled session
/// receives a non-empty control token.
#[tokio::test]
async fn claim_grants_token_when_unheld() {
    // Given — fresh manager, no lease held
    let manager = ClaudeCliSessionManager::new();

    // When
    let outcome = manager.claim_control(SESSION_ID, SCREEN_A, false).await;

    // Then
    match outcome {
        ClaimOutcome::Granted { control_token } => {
            assert!(
                !control_token.is_empty(),
                "granted control token must be non-empty"
            );
        }
        ClaimOutcome::Denied { .. } => {
            panic!("expected Granted for an unheld session, got Denied");
        }
    }
}

/// **claim_without_steal_rejected_when_held_by_other**: a second screen cannot claim without
/// stealing when another screen already holds the lease.
#[tokio::test]
async fn claim_without_steal_rejected_when_held_by_other() {
    // Given — screen A holds the lease
    let manager = ClaudeCliSessionManager::new();
    let _outcome_a = manager.claim_control(SESSION_ID, SCREEN_A, false).await;

    // When — screen B tries to claim without steal
    let outcome_b = manager.claim_control(SESSION_ID, SCREEN_B, false).await;

    // Then
    match outcome_b {
        ClaimOutcome::Denied { holder_screen_id } => {
            assert_eq!(
                holder_screen_id, SCREEN_A,
                "denied response must report the current holder"
            );
        }
        ClaimOutcome::Granted { .. } => {
            panic!("expected Denied for a session held by another screen without steal");
        }
    }
}

/// **claim_with_steal_revokes_previous_holder**: a screen claiming with `steal=true` evicts the
/// previous holder and becomes the new controller.
#[tokio::test]
async fn claim_with_steal_revokes_previous_holder() {
    // Given — screen A holds the lease
    let manager = ClaudeCliSessionManager::new();
    let outcome_a = manager.claim_control(SESSION_ID, SCREEN_A, false).await;
    let token_a = match outcome_a {
        ClaimOutcome::Granted { control_token } => control_token,
        ClaimOutcome::Denied { .. } => panic!("screen A should be granted on fresh session"),
    };

    // When — screen B steals control
    let outcome_b = manager.claim_control(SESSION_ID, SCREEN_B, true).await;

    // Then — screen B is now the controller
    let token_b = match outcome_b {
        ClaimOutcome::Granted { control_token } => {
            assert!(
                !control_token.is_empty(),
                "new controller token must be non-empty"
            );
            control_token
        }
        ClaimOutcome::Denied { .. } => {
            panic!("expected Granted for a steal claim");
        }
    };

    // And screen A's old token is no longer valid
    assert!(
        !manager.verify_control(SESSION_ID, &token_a).await,
        "previous holder's token must be invalidated after steal"
    );

    // And screen B's new token is valid
    assert!(
        manager.verify_control(SESSION_ID, &token_b).await,
        "new holder's token must be valid after steal"
    );
}

/// **verify_control_matches_only_current_token**: `verify_control` returns `true` only for the
/// active token and `false` for any other string.
#[tokio::test]
async fn verify_control_matches_only_current_token() {
    // Given — screen A holds the lease
    let manager = ClaudeCliSessionManager::new();
    let token_a = match manager.claim_control(SESSION_ID, SCREEN_A, false).await {
        ClaimOutcome::Granted { control_token } => control_token,
        ClaimOutcome::Denied { .. } => panic!("unexpected Denied"),
    };

    // Then — valid token accepted
    assert!(
        manager.verify_control(SESSION_ID, &token_a).await,
        "valid token must be accepted"
    );

    // And — wrong token rejected
    assert!(
        !manager.verify_control(SESSION_ID, "wrong-token").await,
        "wrong token must be rejected"
    );

    // And — empty token rejected
    assert!(
        !manager.verify_control(SESSION_ID, "").await,
        "empty token must be rejected"
    );
}

/// **subscribe_emits_change_on_steal**: subscribing then stealing causes the broadcast channel to
/// emit a `ControlChangeEvent` naming the new holder.
#[tokio::test]
async fn subscribe_emits_change_on_steal() {
    // Given — screen A holds; screen B subscribes before stealing
    let manager = ClaudeCliSessionManager::new();
    let _ = manager.claim_control(SESSION_ID, SCREEN_A, false).await;
    let mut rx = manager.subscribe_control();

    // When — screen B steals
    manager.claim_control(SESSION_ID, SCREEN_B, true).await;

    // Then — a change event arrives with session_id and new holder
    let event = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
        .await
        .expect("event must arrive within 500ms")
        .expect("broadcast channel must not be closed");

    assert_eq!(
        event.session_id, SESSION_ID,
        "event must identify the affected session"
    );
    assert_eq!(
        event.holder_screen_id, SCREEN_B,
        "event must name the new holder"
    );
}

// ---------------------------------------------------------------------------
// RPC-level: ConnectionService enforces the control lease.
// ---------------------------------------------------------------------------

/// **send_terminal_input_rejected_without_valid_control_token**: `SendTerminalInput` with an
/// empty or wrong control token is rejected with `FAILED_PRECONDITION` once a controller holds
/// the lease.
#[tokio::test]
#[serial_test::serial]
async fn send_terminal_input_rejected_without_valid_control_token() {
    // Given — a connected session, screen A holds control
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    // Claim control on the manager directly (simulates screen A having the lease)
    let _ = manager.claim_control(SESSION_ID, SCREEN_A, false).await;
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When — send input with wrong/empty control token
    let err = service
        .send_terminal_input(Request::new(SessionTerminalInput {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            data: vec![b'x'],
            terminal_id: String::new(), // → "main"
            control_token: "wrong-token".to_string(),
        }))
        .await
        .expect_err("SendTerminalInput with invalid control token must be rejected");

    // Then
    assert_eq!(
        err.code,
        Code::FailedPrecondition,
        "input from a non-controller must be rejected with FailedPrecondition; got {:?}",
        err.code
    );
}

/// **send_terminal_input_succeeds_for_current_controller**: the screen that holds the lease
/// can send input successfully.
#[tokio::test]
#[serial_test::serial]
async fn send_terminal_input_succeeds_for_current_controller() {
    // Given — screen A holds the lease and knows its token
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let _worktree = start_main_terminal(&manager).await;
    let token = match manager.claim_control(SESSION_ID, SCREEN_A, false).await {
        ClaimOutcome::Granted { control_token } => control_token,
        ClaimOutcome::Denied { .. } => panic!("unexpected Denied on fresh session"),
    };
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When — send input with the correct control token
    let resp = service
        .send_terminal_input(Request::new(SessionTerminalInput {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            data: vec![b'h', b'i'],
            terminal_id: String::new(),
            control_token: token,
        }))
        .await
        .expect("SendTerminalInput from the current controller must succeed");

    // Then
    let _ = resp.into_inner();
}

/// **claim_terminal_control_rpc_returns_granted_for_unheld_session**: calling the
/// `ClaimTerminalControl` RPC on an uncontrolled session grants a non-empty token.
#[tokio::test]
async fn claim_terminal_control_rpc_returns_granted_for_unheld_session() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When
    let resp = service
        .claim_terminal_control(Request::new(ClaimTerminalControlRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            screen_id: SCREEN_A.to_string(),
            steal: false,
        }))
        .await
        .expect("ClaimTerminalControl on an uncontrolled session must succeed")
        .into_inner();

    // Then
    assert!(resp.granted, "must be granted for an uncontrolled session");
    assert!(
        !resp.control_token.is_empty(),
        "granted response must include a non-empty control token"
    );
}

/// **watch_terminal_control_emits_snapshot**: subscribing via `WatchTerminalControl` immediately
/// yields a snapshot event describing the current lease state.
#[tokio::test]
async fn watch_terminal_control_emits_snapshot() {
    // Given — screen A holds the lease via the manager
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let token = match manager.claim_control(SESSION_ID, SCREEN_A, false).await {
        ClaimOutcome::Granted { control_token } => control_token,
        ClaimOutcome::Denied { .. } => panic!("unexpected Denied"),
    };
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When — screen A subscribes to watch
    let mut stream = service
        .watch_terminal_control(Request::new(WatchTerminalControlRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            control_token: token.clone(),
        }))
        .await
        .expect("WatchTerminalControl must succeed")
        .into_inner();

    // Then — first event is a snapshot confirming screen A is the controller
    use futures_util::StreamExt;
    let event = tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
        .await
        .expect("snapshot event must arrive within 500ms")
        .expect("stream must not end immediately")
        .expect("snapshot event must not be an error");

    assert!(
        event.you_are_controller,
        "snapshot must report that the subscribing screen is the controller"
    );
    assert_eq!(
        event.holder_screen_id, SCREEN_A,
        "snapshot must name the controlling screen"
    );
}

/// **terminal_control_rpcs_require_valid_token**: `ClaimTerminalControl` with an invalid
/// session token must return `UNAUTHENTICATED`.
#[tokio::test]
async fn terminal_control_rpcs_require_valid_token() {
    // Given
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let (service, _cfg, _sb) = make_service(Arc::clone(&manager));

    // When — invalid session token
    let err = service
        .claim_terminal_control(Request::new(ClaimTerminalControlRequest {
            session_token: "bad-token".to_string(),
            session_id: SESSION_ID.to_string(),
            screen_id: SCREEN_A.to_string(),
            steal: false,
        }))
        .await
        .expect_err("ClaimTerminalControl with invalid session token must fail");

    // Then
    assert_eq!(
        err.code,
        Code::Unauthenticated,
        "invalid session token must yield UNAUTHENTICATED; got {:?}",
        err.code
    );
}
