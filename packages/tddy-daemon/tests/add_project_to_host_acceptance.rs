//! Acceptance: `AddProjectToHost` makes an existing project available on a target host while
//! reusing the same `project_id`. Local route clones + persists a registry row; the action is
//! idempotent and validates its input (PRD docs/ft/web/projects-screen-multi-host.md).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::{project_storage, user_sessions_path};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    AddProjectToHostRequest, ConnectionService as ConnectionServiceTrait,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const GIVEN_PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(
        ok,
        "git must be available for add_project_to_host acceptance tests"
    );
}

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

/// A source repository the daemon can clone from, given as a local `git_url`.
fn a_source_repo(dir: &Path) -> String {
    require_git();
    std::fs::create_dir_all(dir).unwrap();
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "t@e.st"]);
    run_git(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("README.md"), "x\n").unwrap();
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-m", "init"]);
    dir.to_str().unwrap().to_string()
}

/// Config that clones into `repos_base` (absolute, so it overrides the OS user's home) for the
/// current `$USER`, keeping the test hermetic.
fn test_config(os_user: &str, repos_base: &Path) -> DaemonConfig {
    let yaml = format!(
        r#"
repos_base_path: "{repos}"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#,
        repos = repos_base.display(),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    // Keep the config tempdir alive for the process lifetime — leak is fine in a test binary.
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

fn a_request(project_id: &str, git_url: &str) -> AddProjectToHostRequest {
    AddProjectToHostRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: project_id.to_string(),
        name: "alpha".to_string(),
        git_url: git_url.to_string(),
        main_branch_ref: String::new(),
        // Empty target = local daemon (routes to the local clone/persist branch).
        daemon_instance_id: String::new(),
        user_relative_path: String::new(),
    }
}

#[tokio::test]
async fn add_project_to_host_locally_clones_and_persists_the_row_reusing_the_given_project_id() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source = a_source_repo(&source_dir.path().join("source"));
    let service = test_service(
        test_config(&os_user, repos_base.path()),
        data_dir.path().to_path_buf(),
    );

    // When
    let response = service
        .add_project_to_host(Request::new(a_request(GIVEN_PROJECT_ID, &source)))
        .await
        .expect("add_project_to_host succeeds");

    // Then — the response reuses the given id, not a freshly minted uuid...
    let project = response
        .into_inner()
        .project
        .expect("response carries the project");
    assert_eq!(
        project.project_id, GIVEN_PROJECT_ID,
        "add_project_to_host must reuse the given project_id across hosts"
    );

    // ...and the registry row is persisted with that same id.
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    let stored = project_storage::find_project(&projects_dir, GIVEN_PROJECT_ID)
        .expect("read registry")
        .expect("registry contains the reused project_id");
    assert_eq!(
        stored.git_url, source,
        "persisted row keeps the source git_url"
    );
}

#[tokio::test]
async fn add_project_to_host_is_idempotent_when_the_project_id_already_exists_on_the_target() {
    // Given — the target host already has a row for this project_id
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let projects_dir =
        user_sessions_path::projects_path_for_user(&os_user, Some(data_dir.path())).unwrap();
    project_storage::add_project(
        &projects_dir,
        project_storage::ProjectData {
            project_id: GIVEN_PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .expect("seed existing project");
    let service = test_service(
        test_config(&os_user, repos_base.path()),
        data_dir.path().to_path_buf(),
    );

    // When — adding the same project_id again
    let response = service
        .add_project_to_host(Request::new(a_request(
            GIVEN_PROJECT_ID,
            "https://example.com/alpha.git",
        )))
        .await
        .expect("idempotent add succeeds");

    // Then — returns the existing row and does not create a duplicate
    let project = response
        .into_inner()
        .project
        .expect("response carries the project");
    assert_eq!(project.project_id, GIVEN_PROJECT_ID);
    let rows = project_storage::read_projects(&projects_dir).expect("read registry");
    let matching = rows
        .iter()
        .filter(|p| p.project_id == GIVEN_PROJECT_ID)
        .count();
    assert_eq!(
        matching, 1,
        "idempotent add must not duplicate the registry row"
    );
}

#[tokio::test]
async fn add_project_to_host_rejects_an_empty_project_id() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let service = test_service(
        test_config(&os_user, repos_base.path()),
        data_dir.path().to_path_buf(),
    );

    // When
    let result = service
        .add_project_to_host(Request::new(a_request("", "https://example.com/alpha.git")))
        .await;

    // Then
    let err = result.expect_err("empty project_id must be rejected");
    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "empty project_id must yield invalid_argument, got: {:?}",
        err
    );
}
