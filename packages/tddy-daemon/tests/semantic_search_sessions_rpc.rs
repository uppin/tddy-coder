//! Acceptance: `SearchSessions` RPC returns ranked hits with stable fields (Connect-RPC / ConnectionService).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::changeset::{write_changeset, Changeset, ChangesetState};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::workflow::ids::WorkflowState;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, SearchSessionsRequest,
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
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        None,
        None,
    )
}

/// `search_rpc_returns_stable_schema`: non-empty hits for seeded data; empty query returns empty list; invalid token is unauthenticated.
#[tokio::test]
async fn search_rpc_returns_stable_schema() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "00000000-0007-0000-0000-00000000a001";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let cs = Changeset {
        initial_prompt: Some("RPC fixture: OAuth scopes and refresh tokens".to_string()),
        worktree: Some("/tmp/rpc-wt".to_string()),
        branch: Some("feature/rpc-search".to_string()),
        sessions: vec![tddy_core::changeset::SessionEntry {
            id: session_id.to_string(),
            agent: "claude".to_string(),
            tag: "plan".to_string(),
            created_at: "2026-04-05T12:00:00Z".to_string(),
            system_prompt_file: None,
        }],
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            updated_at: "2026-04-05T12:00:00Z".to_string(),
            history: vec![],
            ..Changeset::default().state
        },
        ..Changeset::default()
    };
    write_changeset(&session_dir, &cs).unwrap();

    tddy_core::session_semantic_search::index_session_for_search(
        &sessions_base,
        session_id,
        &session_dir,
    )
    .expect("index session");

    let service = test_service(sessions_base.clone());

    let empty = service
        .search_sessions(Request::new(SearchSessionsRequest {
            session_token: "valid-token".to_string(),
            query: "   ".to_string(),
        }))
        .await
        .expect("empty query ok");
    assert!(
        empty.into_inner().hits.is_empty(),
        "whitespace-only query must yield empty hits"
    );

    let bad = service
        .search_sessions(Request::new(SearchSessionsRequest {
            session_token: "not-a-token".to_string(),
            query: "oauth".to_string(),
        }))
        .await;
    assert!(bad.is_err(), "invalid session token must fail");

    let res = service
        .search_sessions(Request::new(SearchSessionsRequest {
            session_token: "valid-token".to_string(),
            query: "OAuth refresh".to_string(),
        }))
        .await
        .expect("search ok");
    let inner = res.into_inner();
    assert!(
        !inner.hits.is_empty(),
        "seeded indexed session must appear in search results (non-empty session_id list)"
    );
    let first = &inner.hits[0];
    assert_eq!(first.session_id, session_id);
    assert!(
        !first.initial_prompt.is_empty(),
        "hit must carry initial_prompt for the web client"
    );
    assert!(
        !first.worktree_label.is_empty() || !first.branch_label.is_empty(),
        "hit must include worktree and/or branch labels for display"
    );
}
