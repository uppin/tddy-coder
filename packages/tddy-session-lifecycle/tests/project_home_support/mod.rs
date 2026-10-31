//! The fixture both project-home acceptance suites (`ensure_project_home_session_acceptance`,
//! `project_home_idle_stop_acceptance`) build their daemon from: a real git repo with `origin/main`,
//! a projects registry naming it, a stub `claude` that echoes its argv and stays alive, and a
//! `DaemonSessionHost` serving the current OS user. Each suite keeps its own `World` over these.
//!
//! Every item here is used by both suites, so neither test binary carries dead code.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::GitHubUser;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub const PROJECT_ID: &str = "proj-home";

/// This daemon's signing identity. A main's agent is given a credential of its own, minted under the
/// verified caller, so the daemon must be able to verify the caller's token and sign the agent's.
pub fn this_daemons_session_tokens() -> &'static SessionTokens {
    static TOKENS: OnceLock<SessionTokens> = OnceLock::new();
    TOKENS.get_or_init(|| {
        let home = tempfile::tempdir().expect("a directory for the daemon's key");
        let key = DaemonSigningKey::load_or_generate(&home.path().join("signing_key.pem"))
            .expect("the daemon generates its keypair");
        SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory))
    })
}

/// The credential the browser presents, signed with [`this_daemons_session_tokens`]'s key. Minted
/// once and shared, because the requests and the daemon's user resolver have to agree on the very
/// same string.
pub fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        this_daemons_session_tokens()
            .signer()
            .mint_access(&GitHubUser {
                id: 4242,
                login: current_os_user(),
                avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
                name: "Test User".to_string(),
            })
    })
}

/// A daemon serving one project: what a suite's `World` holds, beside the project's repo.
pub struct HomeDaemon {
    /// Shared, with its sandbox RPC bridge installed as the daemon runtime installs it, so a
    /// sandboxed main's jail can reach it (`#agent-tools 5/12`).
    pub service: Arc<DaemonSessionHost>,
    pub manager: Arc<ClaudeCliSessionManager>,
    pub data: tempfile::TempDir,
    _stub: tempfile::TempDir,
    _cfg: tempfile::TempDir,
}

/// A daemon over one project on `repo` (`origin/main` as its main branch ref) whose registry row
/// carries `extra_row_yaml` (e.g. a `home_session_id:` line), with `extra_config_yaml` appended to
/// its daemon config (e.g. a `project_home:` block).
pub fn a_home_daemon(repo: &Path, extra_row_yaml: &str, extra_config_yaml: &str) -> HomeDaemon {
    let data = tempfile::tempdir().unwrap();
    write_project_registry(data.path(), repo, extra_row_yaml);
    let stub = tempfile::tempdir().unwrap();
    let claude = write_argv_and_env_script(stub.path());
    let (cfg, config) = write_daemon_config(claude.to_str().unwrap(), extra_config_yaml);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service = Arc::new(minimal_service_with_manager(
        config,
        data.path().to_path_buf(),
        Arc::clone(&manager),
    ));
    service.install_sandbox_rpc_bridge();
    HomeDaemon {
        service,
        manager,
        data,
        _stub: stub,
        _cfg: cfg,
    }
}

/// `EnsureProjectHomeSession` for the project, as the caller: with `start`, set main up (or replace
/// it); without, reach the recorded main.
pub async fn ensure_main(
    service: &DaemonSessionHost,
    start: Option<StartSessionRequest>,
) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
    service
        .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
            session_token: a_caller_token().to_string(),
            project_id: PROJECT_ID.to_string(),
            daemon_instance_id: String::new(),
            start,
        }))
        .await
        .map(|r| r.into_inner())
}

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {cwd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A repo on `main` with one commit, pushed to an `origin` that is itself.
pub fn a_repo_with_origin_main() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    git(
        repo.path(),
        &["remote", "add", "origin", repo.path().to_str().unwrap()],
    );
    git(repo.path(), &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn write_project_registry(data: &Path, repo: &Path, extra_row_yaml: &str) {
    let projects = data.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT_ID}\n    name: home\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n{extra_row_yaml}",
            repo.display()
        ),
    )
    .unwrap();
}

fn write_daemon_config(stub_binary: &str, extra_yaml: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: {stub_binary}
{extra_yaml}"#
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
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == a_caller_token()).then(|| resolved_user.clone()));
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
    .with_session_tokens(this_daemons_session_tokens().clone())
}

/// Executable stub that dumps its argv, `TDDY_SOCKET` and the `TDDY_REMOTE_*` transport, then stays alive like an agent
/// waiting for input — so a started main is still running when it is ensured again, and the idle
/// reaper (which only considers live processes) is not racing a process exit. `cat` ends when the
/// session is stopped (SIGTERM) or its terminal closes.
fn write_argv_and_env_script(dir: &Path) -> PathBuf {
    let script_path = dir.join("stub_claude_argv_env.sh");
    std::fs::write(
        &script_path,
        concat!(
            "#!/bin/sh\n",
            "echo \"ARGV: $@\"\n",
            "echo \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\n",
            "echo \"ENVDUMP TDDY_REMOTE_SESSION_ID=[$TDDY_REMOTE_SESSION_ID]\"\n",
            "echo \"ENVDUMP TDDY_REMOTE_SESSION_TOKEN=[$TDDY_REMOTE_SESSION_TOKEN]\"\n",
            "echo \"ENVDUMP TDDY_REMOTE_DAEMON_URL=[$TDDY_REMOTE_DAEMON_URL]\"\n",
            "echo \"ENVDUMP TDDY_REMOTE_DAEMON_SOCKET=[$TDDY_REMOTE_DAEMON_SOCKET]\"\n",
            "exec cat >/dev/null\n",
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    script_path
}
