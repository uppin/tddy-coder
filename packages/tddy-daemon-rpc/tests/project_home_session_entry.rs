//! Acceptance: `ListProjects` reports the session a project records as its main
//! (`ProjectEntry.home_session_id`), and an empty id for a project whose main is not set up.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::Request;
use tddy_service::proto::project::{
    ListProjectsRequest, ProjectEntry, ProjectService as ProjectServiceTrait,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const MAIN_SESSION: &str = "main-0000-0000-0000-000000000000";

fn a_daemon_config(os_user: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

fn a_daemon(config: DaemonConfig, tddy_data_dir: PathBuf) -> TestDaemon {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let eligible: Arc<dyn EligibleDaemonSource> =
        Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config));
    TestDaemon::from_host(DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_daemon_livekit::peer_routing::peer_routing_over(
            &config,
            Some(LiveKitDiscoveryHandles {
                eligible_daemon_source: eligible,
                common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
            }),
        ),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    ))
}

fn projects_dir(data_dir: &Path, os_user: &str) -> PathBuf {
    user_sessions_path::projects_path_for_user(os_user, Some(data_dir)).unwrap()
}

fn a_registered_project(data_dir: &Path, os_user: &str, home_session_id: Option<&str>) {
    project_storage::add_project(
        &projects_dir(data_dir, os_user),
        project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
            home_agent: None,
            home_session_id: home_session_id.map(str::to_string),
        },
    )
    .expect("seed project");
}

async fn the_listed_project(daemon: &TestDaemon) -> ProjectEntry {
    daemon
        .list_projects(Request::direct(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect("list_projects succeeds")
        .into_inner()
        .projects
        .into_iter()
        .find(|p| p.project_id == PROJECT_ID)
        .expect("project is listed")
}

fn os_user() -> String {
    std::env::var("USER").expect("USER must be set for passwd-backed projects path")
}

#[tokio::test]
async fn list_projects_reports_the_recorded_home_session_id() {
    // Given
    let data = tempfile::tempdir().unwrap();
    a_registered_project(data.path(), &os_user(), Some(MAIN_SESSION));
    let daemon = a_daemon(a_daemon_config(&os_user()), data.path().to_path_buf());

    // When
    let listed = the_listed_project(&daemon).await;

    // Then
    assert_eq!(listed.home_session_id, MAIN_SESSION);
}

#[tokio::test]
async fn list_projects_reports_no_home_session_for_a_project_without_main() {
    // Given
    let data = tempfile::tempdir().unwrap();
    a_registered_project(data.path(), &os_user(), None);
    let daemon = a_daemon(a_daemon_config(&os_user()), data.path().to_path_buf());

    // When
    let listed = the_listed_project(&daemon).await;

    // Then
    assert_eq!(listed.home_session_id, "");
}
