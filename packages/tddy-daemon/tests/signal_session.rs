//! Integration tests for SignalSession RPC.
//!
//! These tests verify that the SignalSession method on ConnectionServiceImpl
//! correctly sends OS signals to session processes, rejects dead PIDs, and
//! enforces authentication.
//!
//! Expected to fail (red phase): the SignalSession RPC, Signal enum,
//! SignalSessionRequest, and SignalSessionResponse types do not exist yet.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, Signal, SignalSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config() -> DaemonConfig {
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;
    let path = std::env::temp_dir().join("tddy-signal-session-test-config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let config = test_config();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        None,
        None,
    )
}

fn write_session_yaml(session_dir: &std::path::Path, pid: u32) {
    let session_id = session_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let metadata = SessionMetadata {
        session_id,
        project_id: "proj-1".to_string(),
        created_at: "2026-03-21T10:00:00Z".to_string(),
        updated_at: "2026-03-21T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp".to_string()),
        pid: Some(pid),
        tool: Some("test-tool".to_string()),
        livekit_room: Some("test-room".to_string()),
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Acceptance: SignalSession sends SIGINT to the session's process.
#[tokio::test]
async fn signal_session_sends_sigint_to_pid() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "test-session");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, pid);

    let service = test_service(sessions_base);
    let request = Request::new(SignalSessionRequest {
        session_token: "valid-token".to_string(),
        session_id: "test-session".to_string(),
        signal: Signal::Sigint as i32,
    });
    let response = service.signal_session(request).await.unwrap();
    assert!(response.into_inner().ok);

    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "process should have been terminated by SIGINT"
    );
}

/// Acceptance: SignalSession returns error for a dead PID.
#[tokio::test]
async fn signal_session_returns_error_for_dead_pid() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "dead-session");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, pid);

    let service = test_service(sessions_base);
    let request = Request::new(SignalSessionRequest {
        session_token: "valid-token".to_string(),
        session_id: "dead-session".to_string(),
        signal: Signal::Sigterm as i32,
    });
    let result = service.signal_session(request).await;
    assert!(result.is_err(), "should return error for dead PID");
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::FailedPrecondition);
}

/// Acceptance: SignalSession rejects unauthenticated requests.
#[tokio::test]
async fn signal_session_rejects_unauthenticated_request() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();

    let service = test_service(sessions_base);
    let request = Request::new(SignalSessionRequest {
        session_token: "invalid-token".to_string(),
        session_id: "any-session".to_string(),
        signal: Signal::Sigint as i32,
    });
    let result = service.signal_session(request).await;
    assert!(result.is_err(), "should reject unauthenticated request");
    let status = result.unwrap_err();
    assert_eq!(status.code, tddy_rpc::Code::Unauthenticated);
}
