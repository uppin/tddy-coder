//! Integration-style tests: `ListSessions` returns enriched workflow fields from on-disk fixtures.

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

#[tokio::test]
async fn list_sessions_includes_workflow_fields_from_changeset() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join("enriched-sess-1");
    std::fs::create_dir_all(&session_dir).unwrap();

    let metadata = a_session_metadata()
        .with_session_id("enriched-sess-1")
        .with_project_id("proj-1")
        .with_repo_path("/tmp/repo")
        .with_pid(0)
        .build();
    write_session_yaml(&session_dir, &metadata);
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

    // When
    let response = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions RPC should succeed");

    // Then
    let sessions = response.into_inner().sessions;
    assert_eq!(sessions.len(), 1, "exactly one session must be listed");
    let s = &sessions[0];
    assert_eq!(s.session_id, "enriched-sess-1");
    assert_eq!(s.workflow_goal, "acceptance-tests");
    assert_eq!(s.workflow_state, "Red");
    assert_eq!(s.agent, "claude");
    assert_eq!(s.model, "sonnet-4");
    assert!(!s.elapsed_display.is_empty(), "elapsed_display must not be empty");
    assert_ne!(s.elapsed_display, "—", "elapsed_display must not be the placeholder dash");
}

/// Acceptance: `SessionEntry.pending_elicitation` must match `pending_elicitation` in `.session.yaml`.
#[tokio::test]
async fn list_sessions_sets_pending_elicitation_from_session_metadata() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = sessions_base
        .join(SESSIONS_SUBDIR)
        .join("elicitation-metadata-1");
    std::fs::create_dir_all(&session_dir).unwrap();

    let metadata = a_session_metadata()
        .with_session_id("elicitation-metadata-1")
        .with_project_id("proj-1")
        .with_repo_path("/tmp/repo")
        .with_pid(0)
        .with_pending_elicitation(true)
        .build();
    write_session_yaml(&session_dir, &metadata);
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
    let s = sessions
        .iter()
        .find(|e| e.session_id == "elicitation-metadata-1")
        .expect("session from fixture must be present");
    assert!(
        s.pending_elicitation,
        "ListSessions must set pending_elicitation from authoritative session metadata"
    );
}
