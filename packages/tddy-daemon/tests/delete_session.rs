//! Acceptance tests for DeleteSession RPC (inactive session directory removal).
//!
//! These verify filesystem-safe deletion: inactive sessions, and active sessions after the
//! daemon terminates the recorded PID (SIGTERM then SIGKILL).
//!
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config() -> DaemonConfig {
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
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
    ConnectionServiceImpl::new(config, sessions_base_resolver, user_resolver, None, None)
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

/// Acceptance: DeleteSession removes the on-disk session directory when the session is inactive.
#[tokio::test]
async fn daemon_delete_removes_inactive_session_directory() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "inactive-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, pid);

    let service = test_service(sessions_base);
    let request = Request::new(DeleteSessionRequest {
        session_token: "valid-token".to_string(),
        session_id: "inactive-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should succeed for an inactive session");
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after successful delete"
    );

    let second = Request::new(DeleteSessionRequest {
        session_token: "valid-token".to_string(),
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
async fn daemon_delete_terminates_active_session_then_removes_directory() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "active-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, pid);

    let service = test_service(sessions_base.clone());
    let request = Request::new(DeleteSessionRequest {
        session_token: "valid-token".to_string(),
        session_id: "active-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should terminate the process and remove the directory");
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after delete"
    );

    let wait = child.wait();
    assert!(wait.is_ok(), "child should have been signalled");
}
