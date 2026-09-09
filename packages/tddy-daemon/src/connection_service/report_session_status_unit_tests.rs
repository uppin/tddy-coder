use super::*;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_service::proto::connection::ReportSessionStatusRequest;

const TEST_HOOK_TOKEN: &str = "tok-unit-hook-abc123";
const TEST_OS_USER: &str = "u";

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
    let config = make_unit_config();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |os_user| {
        if os_user == TEST_OS_USER {
            Some(base.clone())
        } else {
            None
        }
    });
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
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

fn write_claude_cli_session(session_dir: &std::path::Path, hook_token: &str) {
    let session_id = session_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let metadata = SessionMetadata {
        session_id,
        project_id: "proj-hook-unit".to_string(),
        created_at: "2026-06-13T10:00:00Z".to_string(),
        updated_at: "2026-06-13T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/hook-test".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-sonnet-4-6".to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: Some(hook_token.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Happy path: valid hook_token, claude-cli session, known status → activity_status written
/// to `.session.yaml`.
#[tokio::test]
async fn report_session_status_writes_activity_status_to_session_yaml() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "hook-writes-status-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

    let service = make_unit_service(sessions_base);
    let request = Request::new(ReportSessionStatusRequest {
        session_id: session_id.to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "Running".to_string(),
    });
    let response = service.report_session_status(request).await.unwrap();
    assert!(response.into_inner().ok, "ok must be true on success");

    let meta = tddy_core::read_session_metadata(&session_dir).unwrap();
    assert_eq!(
        meta.activity_status.as_deref(),
        Some("Running"),
        "activity_status must be written to .session.yaml"
    );
}

/// Missing session → NotFound.
#[tokio::test]
async fn report_session_status_rejects_unknown_session() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::new(ReportSessionStatusRequest {
        session_id: "no-such-session".to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "Running".to_string(),
    });
    let err = service.report_session_status(request).await.unwrap_err();
    assert_eq!(err.code, tddy_rpc::Code::NotFound);
}

/// Wrong hook_token → PermissionDenied.
#[tokio::test]
async fn report_session_status_rejects_bad_hook_token() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "hook-bad-token-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

    let service = make_unit_service(sessions_base);
    let request = Request::new(ReportSessionStatusRequest {
        session_id: session_id.to_string(),
        hook_token: "wrong-token".to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "Running".to_string(),
    });
    let err = service.report_session_status(request).await.unwrap_err();
    assert_eq!(err.code, tddy_rpc::Code::PermissionDenied);
}

/// Non-claude-cli session (tool session) → FailedPrecondition.
#[tokio::test]
async fn report_session_status_rejects_non_claude_cli_session() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "hook-non-cli-session-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Tool session — no session_type = "claude-cli", no hook_token.
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-hook-unit".to_string(),
        created_at: "2026-06-13T10:00:00Z".to_string(),
        updated_at: "2026-06-13T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: None,
        pid: Some(99999),
        tool: Some("tddy-coder".to_string()),
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: None,
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

    let service = make_unit_service(sessions_base);
    let request = Request::new(ReportSessionStatusRequest {
        session_id: session_id.to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "Running".to_string(),
    });
    let err = service.report_session_status(request).await.unwrap_err();
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
}

/// Unknown status string (not in the known set) → InvalidArgument.
#[tokio::test]
async fn report_session_status_rejects_unknown_status_string() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "hook-bad-status-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);

    let service = make_unit_service(sessions_base);
    let request = Request::new(ReportSessionStatusRequest {
        session_id: session_id.to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "UnknownBadStatus".to_string(),
    });
    let err = service.report_session_status(request).await.unwrap_err();
    assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
}

/// Path-traversal in session_id (`../../etc`) → InvalidArgument before any IO.
#[tokio::test]
async fn report_session_status_rejects_session_id_path_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::new(ReportSessionStatusRequest {
        session_id: "../../etc/passwd".to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        status: "Running".to_string(),
    });
    let err = service.report_session_status(request).await.unwrap_err();
    assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
}
