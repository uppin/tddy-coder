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

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

/// Simulates the path that `sessions_base_for_user` returns: `~/.tddy`.
/// `list_sessions_in_dir` appends SESSIONS_SUBDIR ("sessions") to reach session dirs.
fn sessions_base_like_production(tddy_data_dir: &std::path::Path) -> PathBuf {
    tddy_data_dir.to_path_buf()
}

#[tokio::test]
async fn daemon_finds_sessions_created_by_tddy_coder() {
    // Given — simulate ~/.tddy (the data dir that tddy-coder uses)
    let tddy_data_dir = tempfile::tempdir().unwrap();

    // tddy-coder creates sessions at {data_dir}/sessions/{session_id}/.
    let session_dir = tddy_data_dir
        .path()
        .join(SESSIONS_SUBDIR)
        .join("sess-path-test-1");
    std::fs::create_dir_all(&session_dir).unwrap();

    let metadata = a_session_metadata()
        .with_session_id("sess-path-test-1")
        .with_project_id("proj-abc")
        .with_repo_path("/var/tddy/Code/tddy-coder")
        .with_pid(99999)
        .with_tool("tddy-coder")
        .build();
    write_session_yaml(&session_dir, &metadata);

    // The daemon uses sessions_base_for_user which returns ~/.tddy/sessions
    // (i.e., data_dir + "sessions").
    let sessions_base = sessions_base_like_production(tddy_data_dir.path());
    let service = test_service(sessions_base);

    // When
    let response = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions RPC should succeed");

    // Then
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
