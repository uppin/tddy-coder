//! Bug reproduction: sessions created by tddy-coder are invisible to the daemon's ListSessions.
//!
//! `tddy-coder` creates sessions at `{TDDY_DATA_DIR}/sessions/{session_id}/` where
//! `TDDY_DATA_DIR` defaults to `~/.tddy`.
//!
//! The daemon resolves sessions via `sessions_base_for_user` which returns `~/.tddy/sessions`,
//! then `list_sessions_in_dir` joins another `SESSIONS_SUBDIR` ("sessions") →
//! scans `~/.tddy/sessions/sessions/` — one level too deep.
//!
//! Result: sessions with correct `project_id` exist on disk but the web UI shows
//! "No sessions for this project."

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
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

/// Simulates the path that `sessions_base_for_user` returns: `~/.tddy`.
/// `list_sessions_in_dir` appends SESSIONS_SUBDIR ("sessions") to reach session dirs.
fn sessions_base_like_production(tddy_data_dir: &std::path::Path) -> PathBuf {
    tddy_data_dir.to_path_buf()
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

#[tokio::test]
async fn daemon_finds_sessions_created_by_tddy_coder() {
    // Simulate ~/.tddy (the data dir that tddy-coder uses).
    let tddy_data_dir = tempfile::tempdir().unwrap();

    // tddy-coder creates sessions at {data_dir}/sessions/{session_id}/.
    let session_dir = tddy_data_dir
        .path()
        .join(SESSIONS_SUBDIR)
        .join("sess-path-test-1");
    std::fs::create_dir_all(&session_dir).unwrap();

    let metadata = SessionMetadata {
        session_id: "sess-path-test-1".to_string(),
        project_id: "proj-abc".to_string(),
        created_at: "2026-03-29T14:00:00Z".to_string(),
        updated_at: "2026-03-29T14:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/var/tddy/Code/tddy-coder".to_string()),
        pid: Some(99999),
        tool: Some("tddy-coder".to_string()),
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();

    // The daemon uses sessions_base_for_user which returns ~/.tddy/sessions
    // (i.e., data_dir + "sessions").
    let sessions_base = sessions_base_like_production(tddy_data_dir.path());

    let service = test_service(sessions_base);
    let response = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: "valid-token".to_string(),
        }))
        .await
        .expect("ListSessions RPC should succeed");

    let sessions = response.into_inner().sessions;

    assert_eq!(
        sessions.len(),
        1,
        "daemon must find sessions created by tddy-coder at {{data_dir}}/sessions/{{id}}/; \
         sessions_base_for_user returns {{data_dir}}/sessions which causes \
         list_sessions_in_dir to scan {{data_dir}}/sessions/sessions/ (double nesting)"
    );
    assert_eq!(sessions[0].session_id, "sess-path-test-1");
    assert_eq!(sessions[0].project_id, "proj-abc");
}
