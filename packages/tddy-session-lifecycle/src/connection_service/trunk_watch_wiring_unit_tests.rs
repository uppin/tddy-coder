//! An orchestrator-role session is subscribed to its project's remote trunk when the host is told
//! it is running, and a project whose trunk cannot be read is left unwatched while the session
//! runs on (`#orchestra` 5/9).
//!
//! Feature: docs/ft/daemon/project-concept.md § Watching sessions

use super::launch_ports::LaunchHost;
use super::*;
use crate::test_util::{test_host, TEST_TOKEN, TEST_USER};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a55";
const ORCHESTRATOR: &str = "01a1fdd8-0000-7000-8000-0000000000a1";

fn a_host_with_a_project_at(git_url: &str) -> (DaemonSessionHost, tempfile::TempDir) {
    let data = tempfile::tempdir().expect("data tempdir");
    crate::project_storage::write_projects(
        &data.path().join("projects"),
        &[crate::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "trunk-watch".to_string(),
            git_url: git_url.to_string(),
            main_repo_path: data.path().display().to_string(),
            main_branch_ref: Some("origin/main".to_string()),
            remote_name: None,
            host_repo_paths: Default::default(),
            accounts: Vec::new(),
            local_gh: true,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        }],
    )
    .expect("register project");
    let host = test_host(data.path().to_path_buf()).with_trunk_watch(Arc::new(
        crate::session_notifications::SessionNotificationBus::new(),
    ));
    (host, data)
}

#[test]
fn an_orchestrator_that_starts_is_subscribed_to_its_projects_trunk() {
    // Given a host watching trunks and a project on GitHub that uses the host's gh
    let (host, _data) = a_host_with_a_project_at("https://github.com/acme/app.git");

    // When an orchestrator of the project is running
    host.orchestrator_running(TEST_USER, PROJECT_ID, ORCHESTRATOR, TEST_TOKEN);

    // Then the project's trunk is watched
    let watcher = host.trunk_watcher().expect("the host watches trunks");
    assert_eq!(watcher.live_pollers(), vec![PROJECT_ID.to_string()]);
}

#[test]
fn a_project_with_no_github_remote_is_left_unwatched() {
    // Given a project whose remote is not on GitHub
    let (host, _data) = a_host_with_a_project_at("https://git.example.com/acme/app.git");

    // When an orchestrator of the project is running
    host.orchestrator_running(TEST_USER, PROJECT_ID, ORCHESTRATOR, TEST_TOKEN);

    // Then nothing is watched
    let watcher = host.trunk_watcher().expect("the host watches trunks");
    assert_eq!(watcher.live_pollers(), Vec::<String>::new());
}
