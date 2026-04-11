//! Multi-host daemon selection — acceptance tests from the feature PRD Testing Plan.
//!
//! These assert routing, per-host project paths, and cross-daemon safety.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::DaemonAdvertisement;
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ListEligibleDaemonsRequest,
    ListSessionsRequest, StartSessionRequest,
};

const REMOTE_ACCEPTANCE_ROOM: &str = "acceptance-common-room";
const REMOTE_PEER_INSTANCE_ID: &str = "acceptance-daemon-b";
const REMOTE_LK_API_KEY: &str = "devkey";
const REMOTE_LK_API_SECRET: &str = "secret";
const REMOTE_ROUTING_PROJECT_ID: &str = "remote-routing-proj";

fn write_livekit_daemon_yaml(
    ws_url: &str,
    daemon_instance_id: Option<&str>,
    os_user: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let id_block = daemon_instance_id
        .map(|id| format!("daemon_instance_id: {id}\n"))
        .unwrap_or_default();
    let yaml = format!(
        r#"
{id_block}users:
  - github_user: "testuser"
    os_user: "{os_user}"
allowed_tools:
  - path: /bin/true
    label: t
livekit:
  url: {ws_url}
  api_key: {REMOTE_LK_API_KEY}
  api_secret: {REMOTE_LK_API_SECRET}
  common_room: {REMOTE_ACCEPTANCE_ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

struct RestoreTddyProjectsDirEnv(Option<String>);
impl Drop for RestoreTddyProjectsDirEnv {
    fn drop(&mut self) {
        let key = tddy_daemon::user_sessions_path::TDDY_PROJECTS_DIR_ENV;
        match self.0.take() {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

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
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        None,
        None,
    )
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
        pending_elicitation: false,
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Registry read/write preserves distinct paths for the same project id on host A vs host B.
#[test]
#[serial]
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
#[serial]
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

/// StartSession with a fabricated `daemon_instance_id` must fail with a documented gRPC code and an
/// actionable message (never `UNIMPLEMENTED` for “not found” semantics, and never silent local fallback).
#[tokio::test]
#[serial]
async fn start_session_unknown_daemon_instance_id_returns_clear_error() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).unwrap();
    let service = test_service(sessions_base);
    let request = Request::new(StartSessionRequest {
        session_token: "valid-token".to_string(),
        tool_path: "/bin/true".to_string(),
        project_id: "not-consulted-before-daemon-routing".to_string(),
        agent: String::new(),
        daemon_instance_id: "fabricated-peer-not-in-discovery".to_string(),
        recipe: String::new(),
    });
    let err = service
        .start_session(request)
        .await
        .expect_err("StartSession for unknown / non-connected peer must not succeed");
    assert!(
        matches!(
            err.code,
            Code::FailedPrecondition | Code::InvalidArgument | Code::NotFound
        ),
        "expected failed_precondition, invalid_argument, or not_found for unknown peer; got {:?}: {}",
        err.code,
        err.message
    );
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("unknown")
            || msg.contains("not connected")
            || msg.contains("eligible")
            || msg.contains("peer")
            || msg.contains("daemon")
            || msg.contains("discover"),
        "error message should explain unknown or stale peer; got: {}",
        err.message
    );
}

/// StartSession targeting a discovered peer’s `daemon_instance_id` must return gRPC OK and run on that peer
/// (LiveKit RPC to the peer daemon’s **ConnectionService**).
#[tokio::test]
#[serial]
async fn start_session_remote_daemon_instance_id_routes_to_peer() {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();

    let repo_tmp = tempfile::tempdir().unwrap();
    let repo_path = repo_tmp.path();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo_path)
            .status()
            .expect("git init")
            .success(),
        "git init for acceptance repo"
    );

    let os_user = std::env::var("USER").expect("USER required for spawn identity (passwd entry)");

    let projects_tmp = tempfile::tempdir().unwrap();
    let projects_dir = projects_tmp.path().to_path_buf();
    std::fs::create_dir_all(&projects_dir).unwrap();
    let _restore_projects_env = RestoreTddyProjectsDirEnv(
        std::env::var(tddy_daemon::user_sessions_path::TDDY_PROJECTS_DIR_ENV).ok(),
    );
    std::env::set_var(
        tddy_daemon::user_sessions_path::TDDY_PROJECTS_DIR_ENV,
        projects_dir.as_os_str(),
    );
    let project = tddy_daemon::project_storage::ProjectData {
        project_id: REMOTE_ROUTING_PROJECT_ID.to_string(),
        name: "remote-routing".to_string(),
        git_url: "https://example.invalid/tddy-remote-routing.git".to_string(),
        main_repo_path: repo_path.display().to_string(),
        main_branch_ref: None,
        host_repo_paths: HashMap::new(),
    };
    tddy_daemon::project_storage::write_projects(&projects_dir, &[project]).unwrap();

    let (_tmp_a, path_a) = write_livekit_daemon_yaml(&ws_url, None, &os_user);
    let (_tmp_b, path_b) =
        write_livekit_daemon_yaml(&ws_url, Some(REMOTE_PEER_INSTANCE_ID), &os_user);
    let config_a = DaemonConfig::load(&path_a).unwrap();
    let config_b = DaemonConfig::load(&path_b).unwrap();

    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });

    let sessions_b = tempfile::tempdir().unwrap();
    let base_b = sessions_b.path().to_path_buf();
    let resolver_b: SessionsBaseResolver = Arc::new(move |_| Some(base_b.clone()));
    let service_b = ConnectionServiceImpl::new(
        config_b,
        resolver_b,
        user_resolver.clone(),
        None,
        None,
        None,
    );

    let token_b = livekit
        .generate_token(REMOTE_ACCEPTANCE_ROOM, REMOTE_PEER_INSTANCE_ID)
        .expect("LiveKit token for peer daemon");
    let connection_server = tddy_service::ConnectionServiceServer::new(service_b);
    let participant = LiveKitParticipant::connect(
        &ws_url,
        &token_b,
        connection_server,
        RoomOptions::default(),
        None,
    )
    .await
    .expect("peer daemon joins common room with ConnectionService RPC");
    let adv = DaemonAdvertisement {
        instance_id: REMOTE_PEER_INSTANCE_ID.to_string(),
        label: format!("{REMOTE_PEER_INSTANCE_ID} (this daemon)"),
    };
    participant
        .room()
        .local_participant()
        .set_metadata(serde_json::to_string(&adv).unwrap())
        .await
        .expect("peer publishes daemon advertisement metadata");
    let peer_run = tokio::spawn(async move { participant.run().await });

    let sessions_a = tempfile::tempdir().unwrap();
    let base_a = sessions_a.path().to_path_buf();
    let resolver_a: SessionsBaseResolver = Arc::new(move |_| Some(base_a.clone()));
    let config_arc = Arc::new(config_a.clone());
    let registry = Arc::new(tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
        config_arc.clone(),
        registry.clone(),
        room_slot.clone(),
    );
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        tddy_daemon::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(config_arc, registry),
    );
    let service_a = ConnectionServiceImpl::new(
        config_a,
        resolver_a,
        user_resolver,
        None,
        Some(
            tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles {
                eligible_daemon_source: eligible,
                common_room_livekit_room: room_slot,
            },
        ),
        None,
    );

    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let daemons = service_a
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: "valid-token".to_string(),
                }))
                .await
                .expect("ListEligibleDaemons")
                .into_inner()
                .daemons;
            if daemons
                .iter()
                .any(|d| d.instance_id == REMOTE_PEER_INSTANCE_ID)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("timeout waiting for peer daemon in eligible list");

    let request = Request::new(StartSessionRequest {
        session_token: "valid-token".to_string(),
        tool_path: "/bin/true".to_string(),
        project_id: REMOTE_ROUTING_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: REMOTE_PEER_INSTANCE_ID.to_string(),
        recipe: String::new(),
    });
    let response = service_a.start_session(request).await.unwrap_or_else(|e| {
        panic!(
            "StartSession for eligible remote peer must return OK; got {:?}: {}",
            e.code, e.message
        )
    });
    let inner = response.into_inner();
    assert!(!inner.session_id.is_empty());
    let expected_identity = tddy_daemon::spawner::livekit_server_identity_for_session(
        Some(REMOTE_PEER_INSTANCE_ID),
        &inner.session_id,
    );
    assert_eq!(inner.livekit_server_identity, expected_identity);

    peer_run.abort();
}

/// SessionEntry.daemon_instance_id must be populated for listed sessions.
#[tokio::test]
#[serial]
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
#[serial]
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
