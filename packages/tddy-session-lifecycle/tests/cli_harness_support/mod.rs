//! The fixture the CLI-harness daemon suites (`opencode_cli_session`, `warp_cli_session`) build
//! their daemon from: a real git repo with `origin/main`, a projects registry naming it, a daemon
//! config pointing one CLI harness at a stub binary, and a `DaemonSessionHost` serving a test user.
//!
//! The daemon's coding agents are installed as the daemon runtime installs them (see
//! [`coding_agents_over`]): a CLI harness session is launched from its agent's spec, so a session
//! type the host has no agent for is refused.
//!
//! Every item here is used by both suites, so neither test binary carries dead code.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::load_agent_registry;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The session token the host accepts, as the current OS user.
pub const VALID_TOKEN: &str = "valid-token";

/// The OS user the test process runs as — a real, resolvable user (same-user, so the harness spawn
/// needs no privilege drop). Fixtures use this rather than a fabricated name so impersonation
/// resolves during the spawn, as the claude-cli acceptance suite has always done: a generic CLI
/// harness launch resolves the session's OS user for the child's `HOME`/`PATH`.
pub fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A daemon config whose `session_type` harness runs `binary`.
pub fn daemon_config_with_cli_harness_binary(session_type: &str, binary: &Path) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
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
    tddy_session_lifecycle::test_util::daemon_config_from_fixture_file(&config_path)
        .expect("config must parse")
}

/// A session host over `sessions_base` serving [`VALID_TOKEN`], with `manager` running its PTYs and
/// the coding agents of [`coding_agents_over`] installed.
pub fn session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<CliSessionManager>,
) -> DaemonSessionHost {
    let agents = coding_agents_over(&sessions_base);
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
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
    .with_coding_agents(agents)
}

/// The coding agents a daemon keeping its state in `tddy_home` launches from, as the daemon runtime
/// binds them: the agent registry loaded from `<tddyhome>/tools`, resolved against a host whose `PATH`
/// holds the real `tddy-tools` and whose `$HOME` is a directory of the test's. The harness binary
/// itself is the daemon config's `cli_harnesses.<session type>.binary_path`.
pub fn coding_agents_over(tddy_home: &Path) -> Arc<HostCodingAgents> {
    let tools_dir = tddy_home.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    let registry = load_agent_registry(&tools_dir).expect("the agent registry loads");
    let tool_bin = tddy_home.join("tool-bin");
    std::fs::create_dir_all(&tool_bin).unwrap();
    match std::os::unix::fs::symlink(tddy_tools_binary(), tool_bin.join("tddy-tools")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => panic!("tddy-tools is linked into the host's PATH: {e}"),
    }
    let home = tddy_home.join("host-home");
    std::fs::create_dir_all(&home).unwrap();
    Arc::new(HostCodingAgents::new(
        registry,
        Arc::new(AFixtureHost {
            path: vec![tool_bin],
            home,
        }),
    ))
}

/// The real `tddy-tools`, built beside this test binary.
pub fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// A host with a declared `PATH` and `$HOME` over the real filesystem — nothing of the real machine's
/// `PATH`, `$HOME` or `TDDY_*` variables reaches resolution.
struct AFixtureHost {
    path: Vec<PathBuf>,
    home: PathBuf,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        (name == "HOME").then(|| self.home.to_string_lossy().into_owned())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
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
