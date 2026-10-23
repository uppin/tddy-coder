//! Acceptance: an idle project-home session is stopped once it has been idle for
//! `project_home.idle_stop_secs`, keeps its conversation for the next `EnsureProjectHomeSession`,
//! and nothing else is ever auto-stopped.
//!
//! "Idle" is the hook-reported `activity_status` other than `Running`, since the metadata's
//! `updated_at` — which every status report bumps.
//!
//! Feature: docs/ft/daemon/project-concept.md § Project-home auto-stop (`#project-home` 6/7).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, ListSessionsRequest,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const PROJECT_ID: &str = "proj-home";
const HOME_AGENT: &str = "home-claude";
const CURSOR_AGENT: &str = "home-cursor";
const TEN_MINUTES: u64 = 600;

fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn write_config_with_claude_cli_binary(
    stub_binary: &str,
    idle_stop_secs: u64,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: {stub_binary}
project_home:
  idle_stop_secs: {idle_stop_secs}
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
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        None,
        manager,
    )
}

/// Executable stub that dumps both its argv and `TDDY_SOCKET`, then stays alive like a real agent
/// waiting for input — the reaper only considers sessions whose process is running, so a stub that
/// exited at once would make every assertion here race the process exit. `cat` ends when the
/// session is stopped (SIGTERM) or its terminal closes.
fn write_argv_and_env_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_argv_env.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho \"ARGV: $@\"\necho \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\nexec cat >/dev/null\n",
    )
    .unwrap();
    make_executable(&script_path);
    script_path
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
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

/// A daemon whose data dir holds one project (`origin/main` as its main branch ref) with a
/// claude-cli harness profile and a cursor-cli one, and a stub `claude` that echoes its argv.
struct World {
    service: DaemonSessionHost,
    manager: Arc<ClaudeCliSessionManager>,
    _repo: tempfile::TempDir,
    data: tempfile::TempDir,
    _stub: tempfile::TempDir,
    _cfg: tempfile::TempDir,
}

impl World {
    async fn ensure(&self) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.service
            .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }
}

fn a_daemon_with_a_project(home_agent: Option<&str>, idle_stop_secs: u64) -> World {
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

    let data = tempfile::tempdir().unwrap();
    let projects = data.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let home_agent_line = home_agent
        .map(|a| format!("    home_agent: {a}\n"))
        .unwrap_or_default();
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT_ID}\n    name: home\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n{home_agent_line}",
            repo.path().display()
        ),
    )
    .unwrap();
    let agents = data.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("home-claude.yaml"),
        format!("name: {HOME_AGENT}\nharness: claude-cli\nmodel: claude-opus-4-8\n"),
    )
    .unwrap();
    std::fs::write(
        agents.join("home-cursor.yaml"),
        format!("name: {CURSOR_AGENT}\nharness: cursor-cli\nmodel: auto\n"),
    )
    .unwrap();

    let stub = tempfile::tempdir().unwrap();
    let claude = write_argv_and_env_script(stub.path());
    let (cfg, config) =
        write_config_with_claude_cli_binary(claude.to_str().unwrap(), idle_stop_secs);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service =
        minimal_service_with_manager(config, data.path().to_path_buf(), Arc::clone(&manager));
    World {
        service,
        manager,
        _repo: repo,
        data,
        _stub: stub,
        _cfg: cfg,
    }
}

impl World {
    /// Record a hook status report on `session_id`, now — as the `ReportSessionStatus` handler
    /// does (it also bumps `updated_at`).
    fn reported(&self, session_id: &str, activity_status: &str) {
        tddy_core::update_activity_status(
            &self.data.path().join("sessions").join(session_id),
            activity_status,
        )
        .expect("record the status report");
    }

    /// Run the reaper as if `later` had passed since now.
    async fn reap_after(&self, later: Duration) -> Vec<String> {
        self.service
            .reap_idle_project_home_sessions(SystemTime::now() + later)
            .await
    }

    async fn is_active(&self, session_id: &str) -> bool {
        self.service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions")
            .into_inner()
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .expect("listed")
            .is_active
    }

    async fn an_ordinary_claude_session(&self) -> String {
        self.service
            .start_session(Request::direct(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                session_type: "claude-cli".to_string(),
                model: "claude-opus-4-8".to_string(),
                ..Default::default()
            }))
            .await
            .expect("an ordinary session starts")
            .into_inner()
            .session_id
    }
}

const ELEVEN_MINUTES: Duration = Duration::from_secs(11 * 60);

#[tokio::test]
async fn an_idle_project_home_session_is_stopped_after_the_configured_time() {
    // Given a home session waiting for input for eleven minutes, with a ten-minute limit
    let world = a_daemon_with_a_project(Some(HOME_AGENT), TEN_MINUTES);
    let home = world
        .ensure()
        .await
        .expect("ensure")
        .session
        .unwrap()
        .session_id;
    world.reported(&home, "WaitingForInput");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert_eq!(stopped, vec![home.clone()]);
    assert!(!world.is_active(&home).await);
}

#[tokio::test]
async fn a_project_home_session_mid_turn_is_not_stopped() {
    // Given a home session whose turn has been running for eleven minutes
    let world = a_daemon_with_a_project(Some(HOME_AGENT), TEN_MINUTES);
    let home = world
        .ensure()
        .await
        .expect("ensure")
        .session
        .unwrap()
        .session_id;
    world.reported(&home, "Running");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&home).await);
}

#[tokio::test]
async fn ensuring_an_auto_stopped_home_session_resumes_the_same_conversation() {
    // Given a home session the reaper stopped
    let world = a_daemon_with_a_project(Some(HOME_AGENT), TEN_MINUTES);
    let home = world
        .ensure()
        .await
        .expect("ensure")
        .session
        .unwrap()
        .session_id;
    world.reported(&home, "WaitingForInput");
    let later = ELEVEN_MINUTES;
    assert_eq!(world.reap_after(later).await, vec![home.clone()]);

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then
    assert_eq!(resumed.outcome, "resumed");
    assert_eq!(resumed.session.unwrap().session_id, home);
    let handle = world
        .manager
        .get(&home)
        .await
        .expect("the agent runs again");
    let output = a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await;
    assert!(
        output.contains(&format!("--resume {home}")),
        "same conversation; got: {output:?}"
    );
}

#[tokio::test]
async fn an_ordinary_session_is_never_auto_stopped() {
    // Given an ordinary claude-cli session idle for eleven minutes
    let world = a_daemon_with_a_project(Some(HOME_AGENT), TEN_MINUTES);
    let ordinary = world.an_ordinary_claude_session().await;
    world.reported(&ordinary, "WaitingForInput");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&ordinary).await);
}

#[tokio::test]
async fn idle_stop_secs_zero_disables_auto_stop() {
    // Given auto-stop switched off and a home session idle for a day
    let world = a_daemon_with_a_project(Some(HOME_AGENT), 0);
    let home = world
        .ensure()
        .await
        .expect("ensure")
        .session
        .unwrap()
        .session_id;
    world.reported(&home, "WaitingForInput");
    let later = Duration::from_secs(24 * 3600);

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&home).await);
}
