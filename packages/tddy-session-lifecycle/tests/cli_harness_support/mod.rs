//! The fixture the CLI-harness daemon suites (`opencode_cli_session`, `warp_cli_session`) build
//! their daemon from: a real git repo with `origin/main`, a projects registry naming it, a daemon
//! config pointing one CLI harness at a stub binary, and a `DaemonSessionHost` serving a test user.
//!
//! Every item here is used by both suites, so neither test binary carries dead code.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The session token the host accepts, as the user `testuser`.
pub const VALID_TOKEN: &str = "valid-token";

/// A daemon config whose `session_type` harness runs `binary`.
pub fn daemon_config_with_cli_harness_binary(session_type: &str, binary: &Path) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
cli_harnesses:
  {session_type}:
    binary_path: {binary}
"#,
        binary = binary.display()
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    DaemonConfig::load(&config_path).expect("config must parse")
}

/// A session host over `sessions_base` serving [`VALID_TOKEN`], with `manager` running its PTYs.
pub fn session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<CliSessionManager>,
) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
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

/// A git repo in `dir` with one empty commit on `main`, pushed to an `origin` that is itself.
pub fn create_test_repo_with_origin(dir: &Path) {
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

/// A projects registry in `projects_dir` naming `repo_path` as project `project_id`.
pub fn register_project(projects_dir: &Path, repo_path: &Path, project_id: &str) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {project_id}\n    name: {project_id}\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}
