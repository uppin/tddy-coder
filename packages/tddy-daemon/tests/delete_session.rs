//! Acceptance tests for DeleteSession RPC (inactive session directory removal).
//!
//! These verify filesystem-safe deletion: inactive sessions, and active sessions after the
//! daemon terminates the recorded PID (SIGTERM then SIGKILL).

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest,
};
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;

/// Acceptance: DeleteSession removes the on-disk session directory when the session is inactive.
#[tokio::test]
async fn removes_inactive_session_directory() {
    // Given
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "inactive-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("inactive-delete-me")
        .with_pid(pid)
        .with_repo_path("/tmp")
        .with_tool("test-tool")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);

    let service = test_service(sessions_base);

    // When
    let request = Request::new(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "inactive-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should succeed for an inactive session");

    // Then
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after successful delete"
    );

    // Second delete should fail — session is gone from this daemon
    let second = Request::new(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "inactive-delete-me".to_string(),
    });
    let repeat = service.delete_session(second).await;
    assert!(
        repeat.is_err(),
        "second delete after removal should fail (ownership / routing)"
    );
    assert_eq!(
        repeat.unwrap_err().code,
        tddy_rpc::Code::FailedPrecondition,
        "missing session on this daemon maps to failed_precondition (multi-host safe)"
    );
}

/// Acceptance: DeleteSession terminates a live PID then removes the session directory.
#[cfg(unix)]
#[tokio::test]
async fn terminates_active_session_then_removes_directory() {
    // Given
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "active-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("active-delete-me")
        .with_pid(pid)
        .with_repo_path("/tmp")
        .with_tool("test-tool")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);

    let service = test_service(sessions_base);

    // When
    let request = Request::new(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "active-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should terminate the process and remove the directory");

    // Then
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after delete"
    );

    let wait = child.wait();
    assert!(wait.is_ok(), "child should have been signalled");
}
