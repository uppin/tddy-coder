use super::*;
use std::fs;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_service::proto::connection::ListSessionsRequest;

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
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
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
        sessions_base.clone(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

#[tokio::test]
async fn list_sessions_unit_returns_new_metadata_fields() {
    let temp = tempfile::tempdir().unwrap();
    let session_id = "list-test-session-001";
    let session_dir = temp.path().join(SESSIONS_SUBDIR).join(session_id);
    fs::create_dir_all(&session_dir).unwrap();

    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "".to_string(),
        created_at: "2026-06-21T10:00:00Z".to_string(),
        updated_at: "2026-06-21T12:00:00Z".to_string(),
        status: "exited".to_string(),
        repo_path: Some("/home/dev/repo".to_string()),
        pid: None,
        tool: Some("tddy-coder".to_string()),
        livekit_room: Some("room-xyz-ct".to_string()),
        pending_elicitation: false,
        previous_session_id: Some("ancestor-session".to_string()),
        session_type: Some("tool".to_string()),
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
    write_session_metadata(&session_dir, &metadata).unwrap();

    let service = make_unit_service(temp.path().to_path_buf());
    let result = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: "valid".to_string(),
        }))
        .await;
    assert!(result.is_ok());
    let sessions = result.unwrap().into_inner().sessions;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].tool, "tddy-coder");
    assert_eq!(sessions[0].session_type, "tool");
    assert_eq!(sessions[0].updated_at, "2026-06-21T12:00:00Z");
    assert_eq!(sessions[0].previous_session_id, "ancestor-session");
    assert_eq!(sessions[0].livekit_room, "room-xyz-ct");
}
