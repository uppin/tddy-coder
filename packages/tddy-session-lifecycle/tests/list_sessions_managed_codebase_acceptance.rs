//! Acceptance: whether a session was started in managed-codebase mode is recorded at start and
//! reported by `ListSessions`, so the web can open the activity sidebar for managed sessions.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § Right sidebar (`#project-home` 3/7).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ListSessionsRequest, SessionEntry, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_PROJECT_ID: &str = "test-project";

/// The OS user the test process runs as — a real, resolvable user (same-user, so the interactive
/// claude-cli spawn needs no privilege drop). Fixtures use this rather than a fabricated name so
/// impersonation resolves during the spawn.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn write_config_with_claude_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: {stub_binary}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service_with_manager(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}

/// Create a git repo with an origin remote pointing at itself so worktree setup succeeds.
fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn a_claude_cli_start(managed_codebase: bool) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-4-8".to_string(),
        managed_codebase,
        sandbox: false,
        ..Default::default()
    }
}

struct World {
    service: DaemonSessionHost,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
    _cfg: tempfile::TempDir,
}

fn a_daemon_with_a_project() -> World {
    let repo = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo.path());
    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo.path());
    let (cfg, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service_with_manager(
        config,
        sessions.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );
    World {
        service,
        _repo: repo,
        _sessions: sessions,
        _cfg: cfg,
    }
}

async fn started(world: &World, managed_codebase: bool) -> String {
    world
        .service
        .start_session(Request::direct(a_claude_cli_start(managed_codebase)))
        .await
        .expect("claude-cli StartSession succeeds")
        .into_inner()
        .session_id
}

async fn the_listed(world: &World, session_id: &str) -> SessionEntry {
    world
        .service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions succeeds")
        .into_inner()
        .sessions
        .into_iter()
        .find(|s| s.session_id == session_id)
        .expect("the started session is listed")
}

#[tokio::test]
async fn a_managed_session_is_listed_as_managed() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let session_id = started(&world, true).await;

    // Then
    assert!(the_listed(&world, &session_id).await.managed_codebase);
}

#[tokio::test]
async fn an_unmanaged_session_is_listed_as_not_managed() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let session_id = started(&world, false).await;

    // Then
    assert!(!the_listed(&world, &session_id).await.managed_codebase);
}

// Whether the session's agent reads this host's filesystem is reported alongside, so a file dropped
// onto the session's terminal in Tddy Desktop can be handed over by its real path rather than
// uploaded. Feature: docs/ft/web/web-terminal.md § File drop upload.

#[tokio::test]
async fn an_unconfined_session_is_listed_as_reading_host_paths() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let session_id = started(&world, false).await;

    // Then
    assert!(the_listed(&world, &session_id).await.reads_host_paths);
}

#[tokio::test]
async fn a_managed_session_is_listed_as_not_reading_host_paths() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let session_id = started(&world, true).await;

    // Then
    assert!(!the_listed(&world, &session_id).await.reads_host_paths);
}
