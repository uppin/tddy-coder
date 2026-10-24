//! Acceptance: `CreateProject` clones a repository as the session's OS user into
//! `<repos base>/<name>`, registers it under a fresh `project_id`, and refuses names that would
//! escape the repos base, missing input, unknown callers and a clone that cannot run.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::project::{
    CreateProjectRequest, ProjectEntry, ProjectService as ProjectServiceTrait,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::user_sessions_path;
use tddy_spawn::supervisor_client::SpawnBackendChoice;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const MAPPED_GITHUB_USER: &str = "testuser";

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
    std::fs::create_dir_all(dir).unwrap();
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "t@e.st"]);
    run_git(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("README.md"), "x\n").unwrap();
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-m", "init"]);
    dir.to_str().unwrap().to_string()
}

fn load_config(yaml: &str) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    // Keep the config tempdir alive for the process lifetime — leak is fine in a test binary.
    std::mem::forget(dir);
    DaemonConfig::load(&path).unwrap()
}

/// The host a test runs against: its data dir, the repos base clones land under, and the OS
/// user `testuser` maps to (the current `$USER`, so the clone needs no privilege change).
struct Host {
    os_user: String,
    data_dir: tempfile::TempDir,
    repos_base: tempfile::TempDir,
    source_dir: tempfile::TempDir,
}

fn a_host() -> Host {
    Host {
        os_user: std::env::var("USER").expect("USER must be set for passwd-backed projects path"),
        data_dir: tempfile::tempdir().unwrap(),
        repos_base: tempfile::tempdir().unwrap(),
        source_dir: tempfile::tempdir().unwrap(),
    }
}

impl Host {
    fn a_source_repo(&self) -> String {
        a_source_repo(&self.source_dir.path().join("source"))
    }

    fn a_missing_source_repo(&self) -> String {
        self.source_dir
            .path()
            .join("no-such-repo")
            .to_str()
            .unwrap()
            .to_string()
    }

    /// A daemon config that names no supervisor. Whether one brokers the clone is decided by how the
    /// daemon was started, which each test hands to [`Self::daemon_for`] directly.
    fn a_config(&self) -> DaemonConfig {
        load_config(&format!(
            r#"
repos_base_path: "{repos}"
users:
  - github_user: "{MAPPED_GITHUB_USER}"
    os_user: "{os_user}"
"#,
            repos = self.repos_base.path().display(),
            os_user = self.os_user,
        ))
    }

    /// A socket no `tddy-supervisor` is listening on.
    fn missing_supervisor_socket(&self) -> PathBuf {
        self.data_dir.path().join("no-supervisor.sock")
    }

    /// The daemon, where `TEST_TOKEN` authenticates as `github_user` and clones through
    /// `spawn_backend`.
    fn daemon_for(
        &self,
        config: DaemonConfig,
        github_user: &'static str,
        spawn_backend: SpawnBackendChoice,
    ) -> TestDaemon {
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
            spawn_backend,
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
        self.daemon_for(
            self.a_config(),
            MAPPED_GITHUB_USER,
            SpawnBackendChoice::ForkedWorker,
        )
    }

    fn clone_destination(&self, name: &str) -> PathBuf {
        self.repos_base.path().join(name)
    }

    fn registered_projects(&self) -> Vec<project_storage::ProjectData> {
        let projects_dir =
            user_sessions_path::projects_path_for_user(&self.os_user, Some(self.data_dir.path()))
                .unwrap();
        project_storage::read_projects(&projects_dir).expect("read registry")
    }
}

fn a_request(name: &str, git_url: &str) -> CreateProjectRequest {
    CreateProjectRequest {
        session_token: TEST_TOKEN.to_string(),
        name: name.to_string(),
        git_url: git_url.to_string(),
        user_relative_path: String::new(),
    }
}

async fn create(
    daemon: &TestDaemon,
    request: CreateProjectRequest,
) -> Result<ProjectEntry, Status> {
    let response = daemon.create_project(Request::direct(request)).await?;
    Ok(response
        .into_inner()
        .project
        .expect("response carries the project"))
}

async fn refusal(daemon: &TestDaemon, request: CreateProjectRequest) -> Code {
    create(daemon, request)
        .await
        .expect_err("CreateProject must be refused")
        .code()
}

#[tokio::test]
async fn create_project_clones_the_repository_into_the_repos_base_under_its_name() {
    // Given
    let host = a_host();
    let source = host.a_source_repo();

    // When
    let project = create(&host.daemon(), a_request("alpha", &source))
        .await
        .expect("create_project succeeds");

    // Then
    let destination = host.clone_destination("alpha").canonicalize().unwrap();
    assert_eq!(
        (
            PathBuf::from(&project.main_repo_path),
            destination.join("README.md").is_file()
        ),
        (destination, true)
    );
}

#[tokio::test]
async fn create_project_registers_the_clone_under_the_returned_project_id() {
    // Given
    let host = a_host();
    let source = host.a_source_repo();

    // When
    let project = create(&host.daemon(), a_request("alpha", &source))
        .await
        .expect("create_project succeeds");

    // Then
    let rows = host.registered_projects();
    assert_eq!(
        rows.iter()
            .map(|p| (p.project_id.as_str(), p.name.as_str(), p.git_url.as_str()))
            .collect::<Vec<_>>(),
        vec![(project.project_id.as_str(), "alpha", source.as_str())]
    );
}

#[tokio::test]
async fn create_project_trims_the_name_and_git_url() {
    // Given
    let host = a_host();
    let source = host.a_source_repo();

    // When
    let project = create(
        &host.daemon(),
        a_request("  alpha  ", &format!("  {source}  ")),
    )
    .await
    .expect("create_project succeeds");

    // Then
    assert_eq!(
        (project.name.as_str(), project.git_url.as_str()),
        ("alpha", source.as_str())
    );
}

#[tokio::test]
async fn create_project_rejects_a_blank_name() {
    // Given
    let host = a_host();

    // When
    let code = refusal(
        &host.daemon(),
        a_request("   ", "https://example.com/alpha.git"),
    )
    .await;

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn create_project_rejects_a_name_containing_a_slash() {
    // Given
    let host = a_host();

    // When
    let code = refusal(
        &host.daemon(),
        a_request("nested/alpha", "https://example.com/alpha.git"),
    )
    .await;

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn create_project_rejects_a_name_that_climbs_out_of_the_repos_base() {
    // Given
    let host = a_host();

    // When
    let code = refusal(
        &host.daemon(),
        a_request("..", "https://example.com/alpha.git"),
    )
    .await;

    // Then
    assert_eq!(
        (code, host.registered_projects().len()),
        (Code::InvalidArgument, 0)
    );
}

#[tokio::test]
async fn create_project_rejects_a_blank_git_url() {
    // Given
    let host = a_host();

    // When
    let code = refusal(&host.daemon(), a_request("alpha", "   ")).await;

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn create_project_rejects_an_absolute_user_relative_path() {
    // Given
    let host = a_host();
    let request = CreateProjectRequest {
        user_relative_path: "/etc/alpha".to_string(),
        ..a_request("alpha", &host.a_source_repo())
    };

    // When
    let code = refusal(&host.daemon(), request).await;

    // Then
    assert_eq!(
        (code, host.registered_projects().len()),
        (Code::InvalidArgument, 0)
    );
}

#[tokio::test]
async fn create_project_rejects_an_unknown_session() {
    // Given
    let host = a_host();
    let request = CreateProjectRequest {
        session_token: "not-a-session".to_string(),
        ..a_request("alpha", "https://example.com/alpha.git")
    };

    // When
    let code = refusal(&host.daemon(), request).await;

    // Then
    assert_eq!(code, Code::Unauthenticated);
}

#[tokio::test]
async fn create_project_rejects_a_github_user_with_no_os_user() {
    // Given
    let host = a_host();
    let daemon = host.daemon_for(
        host.a_config(),
        "unmapped-github-user",
        SpawnBackendChoice::ForkedWorker,
    );

    // When
    let code = refusal(&daemon, a_request("alpha", "https://example.com/alpha.git")).await;

    // Then
    assert_eq!(code, Code::PermissionDenied);
}

#[tokio::test]
async fn create_project_registers_nothing_when_the_clone_fails() {
    // Given
    let host = a_host();
    let missing = host.a_missing_source_repo();

    // When
    let result = create(&host.daemon(), a_request("alpha", &missing)).await;

    // Then
    result.expect_err("a clone of a missing repository must fail the create");
    assert_eq!(host.registered_projects().len(), 0);
}

#[tokio::test]
async fn create_project_registers_nothing_when_the_declared_supervisor_is_unreachable() {
    // Given a daemon started by a supervisor whose socket is not running
    let host = a_host();
    let daemon = host.daemon_for(
        host.a_config(),
        MAPPED_GITHUB_USER,
        SpawnBackendChoice::Supervisor {
            socket_path: host.missing_supervisor_socket(),
        },
    );

    // When
    let result = create(&daemon, a_request("alpha", &host.a_source_repo())).await;

    // Then
    result.expect_err("an unreachable supervisor must fail the create");
    assert_eq!(
        (
            host.registered_projects().len(),
            host.clone_destination("alpha").exists()
        ),
        (0, false)
    );
}
