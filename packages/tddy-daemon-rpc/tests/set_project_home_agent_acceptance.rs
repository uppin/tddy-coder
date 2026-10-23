//! Acceptance: `SetProjectHomeAgent` stores the agent a project's project-home session runs,
//! `ListProjects` reports it, an empty id clears it, and an id the agent catalog does not list is
//! refused without touching the registry.
//!
//! Spec: docs/ft/web/projects-screen-multi-host.md (§ Home agent) (`#project-home` 1/7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request};
use tddy_service::proto::project::{
    ListProjectsRequest, ProjectEntry, ProjectService as ProjectServiceTrait,
    SetProjectHomeAgentRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

/// The agent catalog this daemon lists: one configured agent and one harness profile.
const CONFIGURED_AGENT: &str = "claude";
const HARNESS_PROFILE: &str = "claude-on-meta";

fn a_daemon_config_listing_claude(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        "users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\nallowed_agents:\n  - id: {CONFIGURED_AGENT}\n    label: Claude\n"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

fn a_harness_profile_in(data_dir: &Path) {
    let agents_dir = data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(
        agents_dir.join("claude-on-meta.yaml"),
        format!(
            "name: {HARNESS_PROFILE}\nlabel: Claude Code · Meta\nharness: claude-cli\nprovider_account: meta\nmodel: muse-wonder-1.0\n"
        ),
    )
    .unwrap();
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

fn a_registered_project(data_dir: &Path, os_user: &str, home_agent: Option<&str>) {
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
            home_agent: home_agent.map(str::to_string),
        },
    )
    .expect("seed project");
}

fn setting_home_agent_to(agent_id: &str) -> Request<SetProjectHomeAgentRequest> {
    Request::direct(SetProjectHomeAgentRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: PROJECT_ID.to_string(),
        home_agent: agent_id.to_string(),
        daemon_instance_id: String::new(),
    })
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

fn the_stored_home_agent(data_dir: &Path, os_user: &str) -> Option<String> {
    project_storage::find_project(&projects_dir(data_dir, os_user), PROJECT_ID)
        .unwrap()
        .expect("project is stored")
        .home_agent
}

fn os_user() -> String {
    std::env::var("USER").expect("USER must be set for passwd-backed projects path")
}

#[tokio::test]
async fn set_project_home_agent_persists_a_listed_agent_and_list_projects_reports_it() {
    // Given
    let os_user = os_user();
    let data_dir = tempfile::tempdir().unwrap();
    a_harness_profile_in(data_dir.path());
    a_registered_project(data_dir.path(), &os_user, None);
    let daemon = a_daemon(
        a_daemon_config_listing_claude(&os_user),
        data_dir.path().to_path_buf(),
    );

    // When
    let response = daemon
        .set_project_home_agent(setting_home_agent_to(HARNESS_PROFILE))
        .await
        .expect("a listed harness profile is accepted")
        .into_inner();

    // Then
    assert_eq!(
        response
            .project
            .expect("response carries the project")
            .home_agent,
        HARNESS_PROFILE
    );
    assert_eq!(
        the_listed_project(&daemon).await.home_agent,
        HARNESS_PROFILE
    );
    assert_eq!(
        the_stored_home_agent(data_dir.path(), &os_user).as_deref(),
        Some(HARNESS_PROFILE)
    );
}

#[tokio::test]
async fn set_project_home_agent_with_an_empty_id_clears_the_setting() {
    // Given
    let os_user = os_user();
    let data_dir = tempfile::tempdir().unwrap();
    a_registered_project(data_dir.path(), &os_user, Some(CONFIGURED_AGENT));
    let daemon = a_daemon(
        a_daemon_config_listing_claude(&os_user),
        data_dir.path().to_path_buf(),
    );

    // When
    daemon
        .set_project_home_agent(setting_home_agent_to(""))
        .await
        .expect("clearing is accepted");

    // Then
    assert_eq!(the_listed_project(&daemon).await.home_agent, "");
    assert_eq!(the_stored_home_agent(data_dir.path(), &os_user), None);
}

#[tokio::test]
async fn set_project_home_agent_refuses_an_agent_the_catalog_does_not_list() {
    // Given
    let os_user = os_user();
    let data_dir = tempfile::tempdir().unwrap();
    a_registered_project(data_dir.path(), &os_user, Some(CONFIGURED_AGENT));
    let daemon = a_daemon(
        a_daemon_config_listing_claude(&os_user),
        data_dir.path().to_path_buf(),
    );

    // When
    let refusal = daemon
        .set_project_home_agent(setting_home_agent_to("no-such-agent"))
        .await
        .expect_err("an unlisted agent id is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(
        the_stored_home_agent(data_dir.path(), &os_user).as_deref(),
        Some(CONFIGURED_AGENT),
        "a refused id leaves the stored agent unchanged"
    );
}
