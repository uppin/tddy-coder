//! Acceptance: `SetProjectDefaultBranch` persists a project's default branch, and `ProjectEntry`
//! surfaces the stored `main_branch_ref` so clients (the Projects UI) can read it back.
//!
//! This file references the not-yet-existing `SetProjectDefaultBranch` RPC and the new
//! `ProjectEntry.main_branch_ref` field, so it fails to compile until the feature is implemented.
//!
//! PRD: docs/ft/web/projects-screen-multi-host.md § Default branch;
//!      docs/ft/coder/git-integration-base-ref.md § API and tooling surface.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::{project_storage, user_sessions_path};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListProjectsRequest,
    SetProjectDefaultBranchRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn test_config(os_user: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\n");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

fn test_service(config: DaemonConfig, tddy_data_dir: PathBuf) -> ConnectionServiceImpl {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let eligible: Arc<dyn EligibleDaemonSource> = Arc::new(StubEligibleDaemonSource);
    ConnectionServiceImpl::new(
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
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

/// Register a legacy project (no stored default branch) directly in the registry.
fn given_a_registered_project(data_dir: &std::path::Path, os_user: &str) {
    let projects_dir = user_sessions_path::projects_path_for_user(os_user, Some(data_dir)).unwrap();
    project_storage::add_project(
        &projects_dir,
        project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        },
    )
    .expect("seed project");
}

fn a_set_request(main_branch_ref: &str) -> SetProjectDefaultBranchRequest {
    SetProjectDefaultBranchRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: PROJECT_ID.to_string(),
        main_branch_ref: main_branch_ref.to_string(),
        daemon_instance_id: String::new(),
    }
}

#[tokio::test]
async fn set_project_default_branch_persists_the_ref_and_list_projects_reports_it() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When — the operator sets the project's default branch to a remote branch
    service
        .set_project_default_branch(Request::new(a_set_request("origin/main")))
        .await
        .expect("set_project_default_branch succeeds");

    // Then — ListProjects surfaces the stored default on the project entry
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
    assert_eq!(entry.main_branch_ref, "origin/main");

    // ...and it is durable in the registry.
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    let stored = project_storage::find_project(&projects_dir, PROJECT_ID)
        .unwrap()
        .unwrap();
    assert_eq!(stored.main_branch_ref.as_deref(), Some("origin/main"));
}

#[tokio::test]
async fn set_project_default_branch_accepts_a_slash_containing_remote_branch() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    service
        .set_project_default_branch(Request::new(a_set_request("origin/release/2025")))
        .await
        .expect("multi-segment remote branch is a legal default");

    // Then
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    let stored = project_storage::find_project(&projects_dir, PROJECT_ID)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.main_branch_ref.as_deref(),
        Some("origin/release/2025")
    );
}

#[tokio::test]
async fn set_project_default_branch_rejects_an_unsafe_ref_without_mutating_the_registry() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    given_a_registered_project(data_dir.path(), &os_user);
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When
    let result = service
        .set_project_default_branch(Request::new(a_set_request("origin/main;rm -rf /")))
        .await;

    // Then — rejected as invalid_argument and the row keeps its previous (unset) default
    let err = result.expect_err("unsafe ref must be rejected");
    assert_eq!(err.code(), Code::InvalidArgument, "got: {err:?}");
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    let stored = project_storage::find_project(&projects_dir, PROJECT_ID)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.main_branch_ref, None,
        "a rejected set must not mutate the stored default"
    );
}

#[tokio::test]
async fn set_project_default_branch_rejects_an_unknown_project() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let service = test_service(test_config(&os_user), data_dir.path().to_path_buf());

    // When — no project is registered
    let result = service
        .set_project_default_branch(Request::new(SetProjectDefaultBranchRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: "does-not-exist".to_string(),
            main_branch_ref: "origin/main".to_string(),
            daemon_instance_id: String::new(),
        }))
        .await;

    // Then
    let err = result.expect_err("unknown project must be rejected");
    assert_eq!(err.code(), Code::NotFound, "got: {err:?}");
}
