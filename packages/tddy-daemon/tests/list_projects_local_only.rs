//! Acceptance: `ListProjects` with `local_only = true` returns only the local registry's rows and
//! skips peer fan-out; with `local_only = false` it merges peer rows. The flag is what breaks the
//! recursion when a daemon fans out to peers (PRD docs/ft/web/projects-screen-multi-host.md).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListProjectsRequest,
    ProjectEntry as ProtoProjectEntry,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PEER_MARKER: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";

/// Contributes peer `ListProjects` rows so the merge path has something to include.
struct PeerProjectsSource;

#[async_trait::async_trait]
impl EligibleDaemonSource for PeerProjectsSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId("server-2".to_string()),
            label: "server-2".to_string(),
        }]
    }

    async fn peer_project_entries(&self, session_token: &str) -> Vec<ProtoProjectEntry> {
        if session_token != TEST_TOKEN {
            return vec![];
        }
        vec![ProtoProjectEntry {
            project_id: PEER_MARKER.to_string(),
            name: "peer-a".to_string(),
            git_url: "https://example.com/a.git".to_string(),
            main_repo_path: "/peer/a".to_string(),
            daemon_instance_id: "server-2".to_string(),
        }]
    }
}

fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

fn test_service(os_user: &str) -> ConnectionServiceImpl {
    let data_dir = tempfile::tempdir().unwrap().path().to_path_buf();
    let sessions_base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    ConnectionServiceImpl::new(
        test_config_for_os_user(os_user),
        sessions_base_resolver,
        data_dir,
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: Arc::new(PeerProjectsSource),
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

fn count_marker_rows(projects: Vec<ProtoProjectEntry>) -> usize {
    projects
        .into_iter()
        .filter(|p| p.project_id == PEER_MARKER)
        .count()
}

#[tokio::test]
async fn list_projects_with_local_only_skips_peer_fan_out() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let service = test_service(&os_user);

    // When
    let response = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect("list_projects succeeds");

    // Then — no peer rows are merged in
    assert_eq!(
        count_marker_rows(response.into_inner().projects),
        0,
        "local_only must return only the local registry's rows and skip peer fan-out"
    );
}

#[tokio::test]
async fn list_projects_without_local_only_merges_peer_rows() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let service = test_service(&os_user);

    // When
    let response = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: false,
        }))
        .await
        .expect("list_projects succeeds");

    // Then — the eligible peer's project row is merged in
    assert_eq!(
        count_marker_rows(response.into_inner().projects),
        1,
        "default ListProjects must merge rows from eligible peer daemons"
    );
}
