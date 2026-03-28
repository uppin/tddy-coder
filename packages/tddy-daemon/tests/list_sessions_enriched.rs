//! Integration-style tests: `ListSessions` returns enriched workflow fields from on-disk fixtures.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest,
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

#[tokio::test]
async fn list_sessions_includes_workflow_fields_from_changeset() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    let session_dir = sessions_base.join("enriched-sess-1");
    std::fs::create_dir_all(&session_dir).unwrap();

    let metadata = SessionMetadata {
        session_id: "enriched-sess-1".to_string(),
        project_id: "proj-1".to_string(),
        created_at: "2026-03-28T10:00:00Z".to_string(),
        updated_at: "2026-03-28T12:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/repo".to_string()),
        pid: Some(0),
        tool: None,
        livekit_room: None,
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

    std::fs::write(
        session_dir.join("changeset.yaml"),
        r"version: 1
models:
  acceptance-tests: sonnet-4
sessions:
  - id: enriched-sess-1
    agent: claude
    tag: acceptance-tests
    created_at: '2026-03-28T10:00:00Z'
state:
  current: Red
  session_id: enriched-sess-1
  updated_at: '2026-03-28T12:00:00Z'
  history:
    - state: Init
      at: '2026-03-28T11:00:00Z'
    - state: Red
      at: '2026-03-28T12:00:00Z'
",
    )
    .unwrap();

    let service = test_service(sessions_base);
    let response = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: "valid-token".to_string(),
        }))
        .await
        .expect("ListSessions RPC should succeed");
    let sessions = response.into_inner().sessions;
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.session_id, "enriched-sess-1");
    assert_eq!(s.workflow_goal, "acceptance-tests");
    assert_eq!(s.workflow_state, "Red");
    assert_eq!(s.agent, "claude");
    assert_eq!(s.model, "sonnet-4");
    assert!(!s.elapsed_display.is_empty());
    assert_ne!(s.elapsed_display, "—");
}
