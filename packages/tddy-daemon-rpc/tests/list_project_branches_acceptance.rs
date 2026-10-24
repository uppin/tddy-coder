//! Acceptance: `ListProjectBranches` lists a registered project's remote branches under the
//! remote its main checkout tracks, answers that remote as `default_remote`, and refuses a missing
//! `project_id`, an unknown project, a working copy that is not on disk, and unknown callers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::project::{
    ListProjectBranchesRequest, ListProjectBranchesResponse, ProjectService as ProjectServiceTrait,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const MAPPED_GITHUB_USER: &str = "testuser";

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

/// A source repository with a `main` and a `feature/one` branch.
fn a_source_repo_with_two_branches(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    run_git(dir, &["init", "--initial-branch=main"]);
    run_git(dir, &["config", "user.email", "t@e.st"]);
    run_git(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("README.md"), "x\n").unwrap();
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-m", "init"]);
    run_git(dir, &["branch", "feature/one"]);
}

/// The host a test runs against, where `TEST_TOKEN` authenticates as `testuser`, mapped to the
/// current `$USER`.
struct Host {
    os_user: String,
    data_dir: tempfile::TempDir,
    repos: tempfile::TempDir,
}

fn a_host() -> Host {
    Host {
        os_user: std::env::var("USER").expect("USER must be set for passwd-backed projects path"),
        data_dir: tempfile::tempdir().unwrap(),
        repos: tempfile::tempdir().unwrap(),
    }
}

impl Host {
    fn config(&self) -> DaemonConfig {
        let yaml = format!(
            r#"
repos_base_path: "{repos}"
users:
  - github_user: "{MAPPED_GITHUB_USER}"
    os_user: "{os_user}"
"#,
            repos = self.repos.path().display(),
            os_user = self.os_user,
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        // Keep the config tempdir alive for the process lifetime — leak is fine in a test binary.
        std::mem::forget(dir);
        DaemonConfig::load(&path).unwrap()
    }

    fn daemon_for(&self, github_user: &'static str) -> TestDaemon {
        let config = self.config();
        let tddy_data_dir = self.data_dir.path().to_path_buf();
        let sessions_base = tddy_data_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(sessions_base.clone()));
        let user_resolver: UserResolver =
            Arc::new(move |token| (token == TEST_TOKEN).then(|| github_user.to_string()));
        let eligible: Arc<dyn EligibleDaemonSource> =
            Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config));
        TestDaemon::from_host(DaemonSessionHost::new(
            config.clone(),
            sessions_base_resolver,
            tddy_data_dir,
            user_resolver,
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
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

    fn daemon(&self) -> TestDaemon {
        self.daemon_for(MAPPED_GITHUB_USER)
    }

    fn register(&self, main_repo_path: &Path) {
        let projects_dir =
            user_sessions_path::projects_path_for_user(&self.os_user, Some(self.data_dir.path()))
                .unwrap();
        project_storage::add_project(
            &projects_dir,
            ProjectData {
                project_id: PROJECT_ID.to_string(),
                name: "alpha".to_string(),
                git_url: "https://example.com/alpha.git".to_string(),
                main_repo_path: main_repo_path.to_string_lossy().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: HashMap::new(),
                accounts: Vec::new(),
                local_gh: false,
                home_session_id: None,
            },
        )
        .expect("register the project");
    }

    /// Registers a project whose main checkout is a clone tracking its source as `upstream`.
    fn with_a_project_cloned_from_upstream(self) -> Self {
        let source = self.repos.path().join("source");
        a_source_repo_with_two_branches(&source);
        let checkout = self.repos.path().join("alpha");
        run_git(
            self.repos.path(),
            &[
                "clone",
                "--origin",
                "upstream",
                source.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        self.register(&checkout);
        self
    }

    fn with_a_project_whose_working_copy_is_missing(self) -> Self {
        let missing = self.repos.path().join("gone");
        self.register(&missing);
        self
    }

    fn with_a_project_whose_working_copy_is_not_a_repository(self) -> Self {
        let plain_dir = self.repos.path().join("plain");
        std::fs::create_dir_all(&plain_dir).unwrap();
        self.register(&plain_dir);
        self
    }
}

fn a_request(project_id: &str) -> ListProjectBranchesRequest {
    ListProjectBranchesRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: project_id.to_string(),
        daemon_instance_id: String::new(),
    }
}

async fn list_branches(
    daemon: &TestDaemon,
    request: ListProjectBranchesRequest,
) -> Result<ListProjectBranchesResponse, Status> {
    Ok(daemon
        .list_project_branches(Request::direct(request))
        .await?
        .into_inner())
}

async fn refusal(daemon: &TestDaemon, request: ListProjectBranchesRequest) -> Code {
    list_branches(daemon, request)
        .await
        .expect_err("ListProjectBranches must be refused")
        .code()
}

#[tokio::test]
async fn list_project_branches_lists_every_branch_under_the_tracked_remote() {
    // Given
    let host = a_host().with_a_project_cloned_from_upstream();

    // When
    let response = list_branches(&host.daemon(), a_request(PROJECT_ID))
        .await
        .expect("list_project_branches succeeds");

    // Then
    let mut branches = response.branches;
    branches.sort();
    assert_eq!(branches, vec!["upstream/feature/one", "upstream/main"]);
}

#[tokio::test]
async fn list_project_branches_answers_the_tracked_remote_as_the_default_remote() {
    // Given
    let host = a_host().with_a_project_cloned_from_upstream();

    // When
    let response = list_branches(&host.daemon(), a_request(PROJECT_ID))
        .await
        .expect("list_project_branches succeeds");

    // Then
    assert_eq!(response.default_remote, "upstream");
}

#[tokio::test]
async fn list_project_branches_accepts_a_project_id_padded_with_whitespace() {
    // Given
    let host = a_host().with_a_project_cloned_from_upstream();

    // When
    let response = list_branches(&host.daemon(), a_request(&format!("  {PROJECT_ID}  ")))
        .await
        .expect("list_project_branches succeeds");

    // Then
    assert_eq!(response.default_remote, "upstream");
}

#[tokio::test]
async fn list_project_branches_rejects_a_blank_project_id() {
    // Given
    let host = a_host();

    // When
    let code = refusal(&host.daemon(), a_request("   ")).await;

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn list_project_branches_reports_an_unknown_project_as_not_found() {
    // Given
    let host = a_host();

    // When
    let code = refusal(&host.daemon(), a_request(PROJECT_ID)).await;

    // Then
    assert_eq!(code, Code::NotFound);
}

#[tokio::test]
async fn list_project_branches_rejects_a_project_whose_working_copy_is_missing() {
    // Given
    let host = a_host().with_a_project_whose_working_copy_is_missing();

    // When
    let code = refusal(&host.daemon(), a_request(PROJECT_ID)).await;

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn list_project_branches_fails_when_the_working_copy_is_not_a_repository() {
    // Given
    let host = a_host().with_a_project_whose_working_copy_is_not_a_repository();

    // When
    let result = list_branches(&host.daemon(), a_request(PROJECT_ID)).await;

    // Then
    result.expect_err("listing branches of a directory that is not a repository must fail");
}

#[tokio::test]
async fn list_project_branches_rejects_an_unknown_session() {
    // Given
    let host = a_host().with_a_project_cloned_from_upstream();
    let request = ListProjectBranchesRequest {
        session_token: "not-a-session".to_string(),
        ..a_request(PROJECT_ID)
    };

    // When
    let code = refusal(&host.daemon(), request).await;

    // Then
    assert_eq!(code, Code::Unauthenticated);
}

#[tokio::test]
async fn list_project_branches_rejects_a_github_user_with_no_os_user() {
    // Given
    let host = a_host().with_a_project_cloned_from_upstream();

    // When
    let code = refusal(
        &host.daemon_for("unmapped-github-user"),
        a_request(PROJECT_ID),
    )
    .await;

    // Then
    assert_eq!(code, Code::PermissionDenied);
}
