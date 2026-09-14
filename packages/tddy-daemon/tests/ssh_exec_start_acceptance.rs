//! Acceptance: StartSession with `ssh_config_host` materializes the worktree on the SSH target
//! and persists the alias on `.session.yaml`.
//!
//! Empty alias is today's local worktree. A set alias never falls back to LocalShell.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::read_session_metadata;
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::DaemonSessionHost;
use tddy_rpc::Request;
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";
const SSH_ALIAS: &str = "buildbox";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

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
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    )
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git command failed");
    };
    let author_env = |cmd: &mut std::process::Command| {
        cmd.env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    let mut commit = std::process::Command::new("git");
    commit
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(dir);
    author_env(&mut commit);
    commit.output().expect("commit");
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    let mut push = std::process::Command::new("git");
    push.args(["push", "-u", "origin", "main"]).current_dir(dir);
    author_env(&mut push);
    push.output().expect("push");
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

fn ssh_exec_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        managed_codebase: true,
        sandbox: false,
        ssh_config_host: SSH_ALIAS.to_string(),
        ..Default::default()
    }
}

/// StartSession with an SSH Host alias records that alias and a remote worktree path — not a
/// local `.worktrees/` checkout on this daemon.
#[tokio::test]
async fn start_with_ssh_config_host_materializes_a_remote_worktree_path_on_metadata() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    let session_id = service
        .start_session(Request::new(ssh_exec_start_request()))
        .await
        .expect("StartSession with ssh_config_host must succeed")
        .into_inner()
        .session_id;

    // Then
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let metadata = read_session_metadata(&session_dir).expect(".session.yaml must exist");
    assert_eq!(
        metadata.ssh_config_host.as_deref(),
        Some(SSH_ALIAS),
        "the alias must be persisted so exec and resume keep using RemoteShell"
    );
    let repo_path = metadata
        .repo_path
        .as_deref()
        .expect("a remote worktree path must be recorded");
    assert!(
        !repo_path.starts_with(repo_dir.path().to_string_lossy().as_ref()),
        "the worktree must not be the local project checkout; got {repo_path}"
    );
    assert!(
        repo_path.contains(".worktrees") || repo_path.starts_with('/'),
        "the recorded path is the worktree on the SSH target; got {repo_path}"
    );
}
