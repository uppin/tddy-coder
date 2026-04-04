//! Multi-host daemon selection — acceptance tests from the feature PRD Testing Plan.
//!
//! These assert routing, per-host project paths, and cross-daemon safety.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ListEligibleDaemonsRequest,
    ListSessionsRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config() -> DaemonConfig {
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(yaml.as_bytes()).unwrap();
    DaemonConfig::load(tmp.path()).unwrap()
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

fn write_exited_session(session_dir: &std::path::Path, session_id: &str, pid: u32) {
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-1".to_string(),
        created_at: "2026-03-21T10:00:00Z".to_string(),
        updated_at: "2026-03-21T10:00:00Z".to_string(),
        status: "exited".to_string(),
        repo_path: Some("/tmp".to_string()),
        pid: Some(pid),
        tool: Some("tddy-coder".to_string()),
        livekit_room: Some("room".to_string()),
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Registry read/write preserves distinct paths for the same project id on host A vs host B.
#[test]
fn per_host_project_path_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let projects_dir = temp.path().join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();

    let mut host_repo_paths = HashMap::new();
    host_repo_paths.insert("host-a".to_string(), "/home/alice/repos/app".to_string());
    host_repo_paths.insert("host-b".to_string(), "/home/bob/work/app".to_string());

    let project = tddy_daemon::project_storage::ProjectData {
        project_id: "proj-same-id".to_string(),
        name: "app".to_string(),
        git_url: "https://github.com/org/repo.git".to_string(),
        main_repo_path: "/legacy/or/default/path".to_string(),
        main_branch_ref: None,
        host_repo_paths,
    };
    tddy_daemon::project_storage::write_projects(&projects_dir, &[project]).unwrap();

    let path_a = tddy_daemon::project_storage::main_repo_path_for_host(
        &projects_dir,
        "proj-same-id",
        "host-a",
    )
    .unwrap();
    let path_b = tddy_daemon::project_storage::main_repo_path_for_host(
        &projects_dir,
        "proj-same-id",
        "host-b",
    )
    .unwrap();

    assert_eq!(
        path_a.as_deref(),
        Some("/home/alice/repos/app"),
        "host A must resolve to host A checkout path"
    );
    assert_eq!(
        path_b.as_deref(),
        Some("/home/bob/work/app"),
        "host B must resolve to host B checkout path"
    );
}

/// StartSession (or equivalent) with a selected daemon instance must yield a `livekit_server_identity`
/// that incorporates that instance and the new session id (browser / terminal routing).
#[test]
fn start_session_targets_selected_daemon_identity() {
    let session_id = "sess-new-7f3a";
    let selected = "host-west";
    let expected = format!("daemon-{selected}-{session_id}");
    let got = tddy_daemon::spawner::livekit_server_identity_for_session(Some(selected), session_id);
    assert_eq!(
        got, expected,
        "livekit_server_identity must match the selected daemon naming scheme"
    );
}

/// ListEligibleDaemons must return at least the local daemon with `is_local: true`.
#[tokio::test]
async fn list_eligible_daemons_returns_local_daemon() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();
    let service = test_service(sessions_base);
    let request = Request::new(ListEligibleDaemonsRequest {
        session_token: "valid-token".to_string(),
    });
    let response = service
        .list_eligible_daemons(request)
        .await
        .expect("ListEligibleDaemons must not return an error");
    let daemons = &response.into_inner().daemons;
    assert!(!daemons.is_empty(), "must return at least the local daemon");
    assert_eq!(
        daemons.iter().filter(|d| d.is_local).count(),
        1,
        "exactly one daemon must be marked is_local"
    );
    let local = daemons.iter().find(|d| d.is_local);
    let local = local.unwrap();
    assert!(
        !local.instance_id.is_empty(),
        "local daemon instance_id must not be empty"
    );
    assert!(
        !local.label.is_empty(),
        "local daemon label must not be empty"
    );
}

/// SessionEntry.daemon_instance_id must be populated for listed sessions.
#[tokio::test]
async fn session_entry_includes_daemon_instance_id() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "sess-host-check";
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_exited_session(&session_dir, session_id, 99999);

    let service = test_service(sessions_base);
    let request = Request::new(ListSessionsRequest {
        session_token: "valid-token".to_string(),
    });
    let response = service
        .list_sessions(request)
        .await
        .expect("ListSessions must not return an error");
    let sessions = &response.into_inner().sessions;
    assert!(!sessions.is_empty(), "must list the written session");
    let entry = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must be in the list");
    assert!(
        !entry.daemon_instance_id.is_empty(),
        "daemon_instance_id must be populated on listed sessions, got empty string"
    );
}

/// Resume / connect / delete / signal against a daemon that does not own the session must fail with
/// a clear ownership / routing status (not success and not ambiguous empty data).
#[tokio::test]
async fn cross_daemon_session_operation_rejected() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base_a = temp.path().join("sessions_daemon_a");
    let sessions_base_b = temp.path().join("sessions_daemon_b");
    let session_id = "session-owned-by-a-only";
    let session_dir_a = sessions_base_a.join(session_id);
    std::fs::create_dir_all(&session_dir_a).unwrap();
    write_exited_session(&session_dir_a, session_id, pid);

    let service_b = test_service(sessions_base_b);
    let request = Request::new(DeleteSessionRequest {
        session_token: "valid-token".to_string(),
        session_id: session_id.to_string(),
    });
    let err = service_b
        .delete_session(request)
        .await
        .expect_err("delete on non-owning daemon must not succeed");

    assert_eq!(
        err.code,
        tddy_rpc::Code::FailedPrecondition,
        "cross-daemon session control must use failed_precondition (or documented equivalent), not ok"
    );
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("own")
            || msg.contains("daemon")
            || msg.contains("host")
            || msg.contains("routing"),
        "error should explain wrong daemon / ownership; got: {}",
        err.message
    );
}
