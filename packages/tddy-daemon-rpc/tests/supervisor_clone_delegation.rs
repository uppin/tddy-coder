//! What the daemon does when the supervisor it was told to clone a project through is not there.
//!
//! Split from `tddy-session-lifecycle/tests/supervisor_spawn_delegation.rs` with the project
//! handlers: a declared supervisor that cannot be reached fails the clone instead of quietly cloning
//! the repository as the daemon's own user. Background: `docs/ft/supervisor/tddy-supervisor.md`;
//! the fail-closed rule is in `packages/tddy-supervisor/docs/architecture.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use tddy_rpc::Request;
use tddy_service::proto::project::{
    AddProjectToHostRequest, ProjectService as ProjectServiceTrait,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_supervised_config(os_user: &str, repos_base: &Path, missing_socket: &Path) -> DaemonConfig {
    let yaml = format!(
        r#"
repos_base_path: "{repos}"
supervisor:
  socket_path: "{socket}"
livekit:
  url: "ws://127.0.0.1:7880"
  api_key: "test-key"
  api_secret: "test-secret"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#,
        repos = repos_base.display(),
        socket = missing_socket.display(),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    // The config tempdir is only read by `load`; leaking it keeps the test binary simple.
    std::mem::forget(dir);
    DaemonConfig::load(&path).expect("config must parse")
}

fn a_service(config: DaemonConfig, tddy_data_dir: PathBuf) -> TestDaemon {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let eligible: Arc<dyn EligibleDaemonSource> =
        Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config));
    TestDaemon::from_host(DaemonSessionHost::new(
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
    ))
}

#[tokio::test]
async fn refuses_to_clone_a_project_when_the_declared_supervisor_is_unreachable() {
    // Given a host configured for a supervisor that is not running
    let os_user = current_username();
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let sockets = tempfile::tempdir().unwrap();
    let missing_socket = sockets.path().join("tddy-supervisor.sock");
    let service = a_service(
        a_supervised_config(&os_user, repos_base.path(), &missing_socket),
        data_dir.path().to_path_buf(),
    );

    // When
    let error = service
        .add_project_to_host(Request::new(AddProjectToHostRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.invalid/owner/repo.git".to_string(),
            main_branch_ref: String::new(),
            daemon_instance_id: String::new(),
            user_relative_path: String::new(),
        }))
        .await
        .expect_err("an unreachable supervisor must fail the clone");

    // Then the outage is reported, and no working copy was cloned by the daemon itself
    assert!(
        error
            .message()
            .contains(&missing_socket.display().to_string()),
        "the error should name the unreachable supervisor socket, got: {}",
        error.message()
    );
    assert!(
        !repos_base.path().join("alpha").exists(),
        "no repository may be cloned by the daemon itself when a supervisor is configured"
    );
}
