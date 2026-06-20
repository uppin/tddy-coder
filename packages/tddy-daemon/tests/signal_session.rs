//! Integration tests for SignalSession RPC.
//!
//! These tests verify that the SignalSession method on ConnectionServiceImpl
//! correctly sends OS signals to session processes, rejects dead PIDs, and
//! enforces authentication.
//!
//! Expected to fail (red phase): the SignalSession RPC, Signal enum,
//! SignalSessionRequest, and SignalSessionResponse types do not exist yet.

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, Signal, SignalSessionRequest,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

/// Acceptance: SignalSession sends SIGINT to the session's process.
#[tokio::test]
async fn signal_session_sends_sigint_to_pid() {
    // Given
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "test-session");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("test-session")
        .with_pid(pid)
        .build();
    write_session_yaml(&session_dir, &metadata);
    let service = test_service(sessions_base);

    // When
    let response = service
        .signal_session(Request::new(SignalSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "test-session".to_string(),
            signal: Signal::Sigint as i32,
        }))
        .await
        .unwrap();

    // Then
    assert!(
        response.into_inner().ok,
        "signal_session must return ok=true"
    );
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "process should have been terminated by SIGINT"
    );
}

/// Acceptance: SignalSession returns error for a dead PID.
#[tokio::test]
async fn signal_session_returns_error_for_dead_pid() {
    // Given
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "dead-session");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("dead-session")
        .with_pid(pid)
        .build();
    write_session_yaml(&session_dir, &metadata);
    let service = test_service(sessions_base);

    // When
    let result = service
        .signal_session(Request::new(SignalSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "dead-session".to_string(),
            signal: Signal::Sigterm as i32,
        }))
        .await;

    // Then
    assert!(result.is_err(), "should return error for dead PID");
    assert_eq!(
        result.unwrap_err().code,
        tddy_rpc::Code::FailedPrecondition,
        "dead PID must yield failed_precondition"
    );
}

/// Acceptance: SignalSession rejects unauthenticated requests.
#[tokio::test]
async fn signal_session_rejects_unauthenticated_request() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();
    let service = test_service(sessions_base);

    // When
    let result = service
        .signal_session(Request::new(SignalSessionRequest {
            session_token: "invalid-token".to_string(),
            session_id: "any-session".to_string(),
            signal: Signal::Sigint as i32,
        }))
        .await;

    // Then
    assert!(result.is_err(), "should reject unauthenticated request");
    assert_eq!(
        result.unwrap_err().code,
        tddy_rpc::Code::Unauthenticated,
        "invalid token must yield unauthenticated status"
    );
}
