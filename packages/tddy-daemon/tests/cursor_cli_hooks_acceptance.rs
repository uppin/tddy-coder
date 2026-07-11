//! Acceptance tests: `ReportSessionStatus` for cursor-cli hook events.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::claude_cli_session::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{ConnectionServiceImpl, SessionUserResolver};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ReportSessionStatusRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

const TEST_HOOK_TOKEN: &str = "cursor-hook-token-abc";
const TEST_OS_USER: &str = "testuser";

fn minimal_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let yaml =
        format!("users:\n  - github_user: \"{TEST_OS_USER}\"\n    os_user: \"{TEST_OS_USER}\"\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).unwrap();

    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |os_user| {
        if os_user == TEST_OS_USER {
            Some(base.clone())
        } else {
            None
        }
    });
    let user_resolver: SessionUserResolver = Arc::new(|_| Some(TEST_OS_USER.to_string()));

    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn write_cursor_cli_session(session_dir: &std::path::Path, hook_token: &str) {
    let session_id = session_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let metadata = SessionMetadata {
        session_id,
        project_id: "proj-cursor-hook".to_string(),
        created_at: "2026-07-05T10:00:00Z".to_string(),
        updated_at: "2026-07-05T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/cursor-hook".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("cursor-cli".to_string()),
        model: Some("gpt-5.3-codex".to_string()),
        activity_status: None,
        hook_token: Some(hook_token.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

#[tokio::test]
async fn cursor_cli_report_session_status_writes_activity_status() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "cursor-hook-writes-status-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_cursor_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = minimal_service(sessions_base);

    // When
    let response = service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        }))
        .await
        .expect("ReportSessionStatus must succeed for cursor-cli");

    // Then
    assert!(response.into_inner().ok);
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.activity_status.as_deref(), Some("Running"));
}

#[tokio::test]
async fn cursor_cli_report_session_status_rejects_bad_hook_token() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "cursor-hook-bad-token-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_cursor_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = minimal_service(sessions_base);

    // When
    let err = service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: "wrong-token".to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Started".to_string(),
        }))
        .await
        .expect_err("bad hook_token must be rejected");

    // Then
    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn cursor_cli_report_session_status_rejects_tool_session_type() {
    // Given — tool session without cursor-cli session_type
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "cursor-hook-wrong-type-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj".to_string(),
        created_at: "2026-07-05T10:00:00Z".to_string(),
        updated_at: "2026-07-05T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: None,
        pid: Some(1),
        tool: Some("tddy-coder".to_string()),
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: None,
        model: None,
        activity_status: None,
        hook_token: Some(TEST_HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
    let service = minimal_service(sessions_base);

    // When
    let err = service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: TEST_OS_USER.to_string(),
            status: "Running".to_string(),
        }))
        .await
        .expect_err("non-cli session must be rejected");

    // Then
    assert_eq!(err.code(), Code::FailedPrecondition);
}
