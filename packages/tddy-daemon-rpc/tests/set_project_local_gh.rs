//! `SetProjectAccounts` carries a project's **Local gh** setting together with its account
//! assignments, in one replace-write.
//!
//! A Local gh project takes its GitHub identity from each host's own `gh` login, so it has no
//! GitHub account assigned: turning the setting on removes the GitHub assignment, every other
//! provider's assignment stays, and a write naming both GitHub sources is refused rather than one of
//! them silently winning.
//!
//! Feature: docs/ft/daemon/github-identity.md

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::project::{
    AccountAssignment, ListProjectsRequest, ProjectEntry, ProjectService as ProjectServiceTrait,
    SetProjectAccountsRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn an_assignment(provider: &str, account_id: &str) -> AccountAssignment {
    AccountAssignment {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
    }
}

fn a_stored_assignment(provider: &str, account_id: &str) -> project_storage::AccountAssignment {
    project_storage::AccountAssignment {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
    }
}

/// The project's whole GitHub and account setting, as one `SetProjectAccounts` call sends it.
struct ProjectSetting {
    accounts: Vec<AccountAssignment>,
    local_gh: bool,
}

fn using_local_gh() -> ProjectSetting {
    ProjectSetting {
        accounts: Vec::new(),
        local_gh: true,
    }
}

fn using_the_vault() -> ProjectSetting {
    ProjectSetting {
        accounts: Vec::new(),
        local_gh: false,
    }
}

impl ProjectSetting {
    fn assigning(mut self, provider: &str, account_id: &str) -> Self {
        self.accounts.push(an_assignment(provider, account_id));
        self
    }

    fn request(self) -> SetProjectAccountsRequest {
        SetProjectAccountsRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            accounts: self.accounts,
            daemon_instance_id: String::new(),
            local_gh: self.local_gh,
        }
    }
}

/// One daemon, the signed-in `testuser` mapped to the OS user running the suite, and one project.
struct World {
    data_dir: tempfile::TempDir,
    os_user: String,
    daemon: TestDaemon,
}

fn a_daemon_with_a_registered_project() -> World {
    a_daemon_with_a_project_row(Vec::new(), false)
}

/// A daemon whose project row was written as Local gh, also assigned a Cloudflare account.
fn a_daemon_with_a_local_gh_project() -> World {
    a_daemon_with_a_project_row(vec![a_stored_assignment("cloudflare", "acct-edge")], true)
}

fn a_daemon_with_a_project_row(
    accounts: Vec<project_storage::AccountAssignment>,
    local_gh: bool,
) -> World {
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    project_storage::add_project(
        &projects_dir,
        project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://github.com/acme/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts,
            local_gh,
            home_session_id: None,
        },
    )
    .expect("seed project");
    let daemon = a_daemon(&os_user, data_dir.path().to_path_buf());
    World {
        data_dir,
        os_user,
        daemon,
    }
}

fn a_daemon(os_user: &str, tddy_data_dir: PathBuf) -> TestDaemon {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        format!("users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\n"),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).unwrap();
    let sessions_base = tddy_data_dir.clone();
    let eligible: Arc<dyn EligibleDaemonSource> =
        Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config));
    TestDaemon::from_host(DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_data_dir,
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string())),
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

impl World {
    async fn set(&self, setting: ProjectSetting) -> Result<ProjectEntry, Status> {
        self.daemon
            .set_project_accounts(Request::direct(setting.request()))
            .await
            .map(|response| {
                response
                    .into_inner()
                    .project
                    .expect("the response carries the project")
            })
    }

    async fn listed_project(&self) -> ProjectEntry {
        self.daemon
            .list_projects(Request::direct(ListProjectsRequest {
                session_token: TEST_TOKEN.to_string(),
                ..Default::default()
            }))
            .await
            .expect("list_projects succeeds")
            .into_inner()
            .projects
            .into_iter()
            .find(|p| p.project_id == PROJECT_ID)
            .expect("the project is listed")
    }

    fn stored_row(&self) -> project_storage::ProjectData {
        let projects_dir =
            user_sessions_path::projects_path_for_user(&self.os_user, Some(self.data_dir.path()))
                .unwrap();
        project_storage::find_project(&projects_dir, PROJECT_ID)
            .expect("read registry")
            .expect("project is registered")
    }
}

#[tokio::test]
async fn turning_local_gh_on_stores_it_and_list_projects_carries_it() {
    // Given a project that uses the vault
    let world = a_daemon_with_a_registered_project();

    // When Local gh is turned on
    world
        .set(using_local_gh())
        .await
        .expect("the setting is accepted");

    // Then the registry and the listing both say so
    assert!(world.stored_row().local_gh);
    assert!(world.listed_project().await.local_gh);
}

#[tokio::test]
async fn turning_local_gh_on_keeps_every_other_providers_assignment() {
    // Given a project assigned a GitHub account and a Cloudflare account
    let world = a_daemon_with_a_registered_project();
    world
        .set(
            using_the_vault()
                .assigning("github", "acct-octocat")
                .assigning("cloudflare", "acct-edge"),
        )
        .await
        .expect("seed both assignments");

    // When Local gh is turned on, sending the assignments without the GitHub one
    let stored = world
        .set(using_local_gh().assigning("cloudflare", "acct-edge"))
        .await
        .expect("the setting is accepted");

    // Then the Cloudflare assignment is all that remains, beside Local gh
    assert_eq!(
        stored.accounts,
        vec![an_assignment("cloudflare", "acct-edge")]
    );
    assert!(stored.local_gh);
}

#[tokio::test]
async fn a_local_gh_project_assigned_a_github_account_is_refused_naming_both() {
    // Given a project that uses the vault, assigned no account
    let world = a_daemon_with_a_registered_project();

    // When a write names Local gh and a GitHub account together
    let refusal = world
        .set(using_local_gh().assigning("github", "acct-octocat"))
        .await
        .expect_err("a project has one GitHub source");

    // Then it is refused, the reason names both sources, and nothing was stored
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("local gh") && refusal.message().contains("github account"),
        "the refusal must name both GitHub sources, got: {}",
        refusal.message()
    );
    assert!(!world.stored_row().local_gh);
    assert_eq!(world.stored_row().accounts, Vec::new());
}

#[tokio::test]
async fn turning_local_gh_off_leaves_the_project_with_no_github_account() {
    // Given a Local gh project that is also assigned a Cloudflare account
    let world = a_daemon_with_a_local_gh_project();

    // When Local gh is turned off, sending the remaining assignments
    world
        .set(using_the_vault().assigning("cloudflare", "acct-edge"))
        .await
        .expect("the setting is accepted");

    // Then the project uses the vault again, with no GitHub account assigned
    assert!(!world.stored_row().local_gh);
    assert_eq!(
        world.stored_row().accounts,
        vec![a_stored_assignment("cloudflare", "acct-edge")]
    );
}
