//! Acceptance: `SetProjectAccounts` assigns provider accounts to a project, and `ProjectEntry`
//! carries the stored assignments so the Projects screen can render — and change — them.
//!
//! An assignment names both halves, `(provider, account_id)`, because an account id is unique only
//! within its provider. A project with no assignment for a provider resolves to **nothing**: the
//! daemon has no fallback to a caller's own login or to a sole account in the vault.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-assignments.md § Proposed Changes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request};
use tddy_service::proto::project::{
    AccountAssignment, ListProjectsRequest, ProjectService as ProjectServiceTrait,
    SetProjectAccountsRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const OTHER_PROJECT_ID: &str = "99999999-8888-4777-8666-555555555555";

fn test_config(os_user: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

fn test_service(config: DaemonConfig, tddy_data_dir: PathBuf) -> DaemonSessionHost {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let eligible: Arc<dyn EligibleDaemonSource> =
        Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config));
    DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

fn given_a_registered_project(data_dir: &std::path::Path, os_user: &str, project_id: &str) {
    let projects_dir = user_sessions_path::projects_path_for_user(os_user, Some(data_dir)).unwrap();
    project_storage::add_project(
        &projects_dir,
        project_storage::ProjectData {
            project_id: project_id.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts: Vec::new(),
        },
    )
    .expect("seed project");
}

fn an_assignment(provider: &str, account_id: &str) -> AccountAssignment {
    AccountAssignment {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
    }
}

fn a_set_request(accounts: Vec<AccountAssignment>) -> SetProjectAccountsRequest {
    SetProjectAccountsRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: PROJECT_ID.to_string(),
        accounts,
        daemon_instance_id: String::new(),
    }
}

fn stored_accounts(
    data_dir: &std::path::Path,
    os_user: &str,
    project_id: &str,
) -> Vec<project_storage::AccountAssignment> {
    let projects_dir = user_sessions_path::projects_path_for_user(os_user, Some(data_dir)).unwrap();
    project_storage::find_project(&projects_dir, project_id)
        .expect("read registry")
        .expect("project is registered")
        .accounts
}

#[tokio::test]
async fn assigning_an_account_to_a_project_persists_it_and_list_projects_carries_it() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    let assigned = service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-octocat",
        )])))
        .await
        .expect("set_project_accounts succeeds")
        .into_inner()
        .project
        .expect("the response carries the project");

    // Then — the response, the listing and the registry all agree
    assert_eq!(
        assigned.accounts,
        vec![an_assignment("github", "acct-octocat")]
    );

    let listed = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect("list_projects succeeds")
        .into_inner();
    let entry = listed
        .projects
        .into_iter()
        .find(|p| p.project_id == PROJECT_ID)
        .expect("project is listed");
    assert_eq!(
        entry.accounts,
        vec![an_assignment("github", "acct-octocat")]
    );

    assert_eq!(
        stored_accounts(data_dir.path(), &os_user, PROJECT_ID),
        vec![project_storage::AccountAssignment {
            provider: "github".to_string(),
            account_id: "acct-octocat".to_string(),
        }]
    );
}

#[tokio::test]
async fn a_project_nobody_assigned_an_account_to_is_listed_as_unassigned() {
    // Given — a registered project and no assignment at all
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    let listed = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: true,
        }))
        .await
        .expect("list_projects succeeds")
        .into_inner();

    // Then — an empty assignment set, which the screen reads as "no account assigned"
    let entry = listed
        .projects
        .into_iter()
        .find(|p| p.project_id == PROJECT_ID)
        .expect("project is listed");
    assert_eq!(entry.accounts, Vec::new());
}

#[tokio::test]
async fn assigning_a_new_set_replaces_the_previous_assignments_rather_than_merging_them() {
    // Given — a project already assigned one github account
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());
    service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-octocat",
        )])))
        .await
        .expect("seed the first assignment");

    // When — a second call names a different account at the same provider
    service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-hubot",
        )])))
        .await
        .expect("set_project_accounts succeeds");

    // Then — the row carries exactly the new set
    assert_eq!(
        stored_accounts(data_dir.path(), &os_user, PROJECT_ID),
        vec![project_storage::AccountAssignment {
            provider: "github".to_string(),
            account_id: "acct-hubot".to_string(),
        }]
    );
}

#[tokio::test]
async fn two_accounts_at_one_provider_are_refused_and_the_stored_set_is_untouched() {
    // Given — a project assigned one github account
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());
    service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-octocat",
        )])))
        .await
        .expect("seed the first assignment");

    // When — a set naming the same provider twice
    let result = service
        .set_project_accounts(Request::new(a_set_request(vec![
            an_assignment("github", "acct-hubot"),
            an_assignment("github", "acct-octocat"),
        ])))
        .await;

    // Then — refused, naming the provider, and the stored set is exactly as it was
    let err = result.expect_err("a repeated provider must be refused");
    assert_eq!(err.code(), Code::InvalidArgument, "got: {err:?}");
    assert!(
        err.message().contains("github"),
        "the refusal must name the provider, got: {}",
        err.message()
    );
    assert_eq!(
        stored_accounts(data_dir.path(), &os_user, PROJECT_ID),
        vec![project_storage::AccountAssignment {
            provider: "github".to_string(),
            account_id: "acct-octocat".to_string(),
        }]
    );
}

#[tokio::test]
async fn assigning_accounts_to_one_project_leaves_every_other_project_unassigned() {
    // Given — two registered projects
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    given_a_registered_project(data_dir.path(), &os_user, OTHER_PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When — only one of them is assigned an account
    service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-octocat",
        )])))
        .await
        .expect("set_project_accounts succeeds");

    // Then
    assert_eq!(
        stored_accounts(data_dir.path(), &os_user, OTHER_PROJECT_ID),
        Vec::new()
    );
}

#[tokio::test]
async fn assigning_accounts_to_a_project_this_daemon_does_not_know_is_not_found() {
    // Given — no project is registered
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    let result = service
        .set_project_accounts(Request::new(a_set_request(vec![an_assignment(
            "github",
            "acct-octocat",
        )])))
        .await;

    // Then
    let err = result.expect_err("an unknown project must be refused");
    assert_eq!(err.code(), Code::NotFound, "got: {err:?}");
}

#[tokio::test]
async fn assigning_accounts_without_a_valid_session_is_unauthenticated() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user, PROJECT_ID);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    let result = service
        .set_project_accounts(Request::new(SetProjectAccountsRequest {
            session_token: "not-a-session".to_string(),
            project_id: PROJECT_ID.to_string(),
            accounts: vec![an_assignment("github", "acct-octocat")],
            daemon_instance_id: String::new(),
        }))
        .await;

    // Then — refused before the registry is read, and the row stays unassigned
    let err = result.expect_err("an unknown session must be refused");
    assert_eq!(err.code(), Code::Unauthenticated, "got: {err:?}");
    assert_eq!(
        stored_accounts(data_dir.path(), &os_user, PROJECT_ID),
        Vec::new()
    );
}
