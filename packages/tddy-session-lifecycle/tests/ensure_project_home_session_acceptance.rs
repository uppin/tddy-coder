//! Acceptance: `EnsureProjectHomeSession` starts, resumes or returns a project's project-home
//! session — a managed session on a dedicated home worktree detached at the project's main branch
//! ref, running the project's home agent — and refuses when there is no home agent or its harness
//! cannot enforce managed-codebase mode.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session (`#project-home` 4/7).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, ListSessionsRequest,
    SessionService as SessionServiceTrait,
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
const PROJECT_HOME_ROLE: &str = "project-home";

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
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        None,
        manager,
    )
}

/// Executable stub that dumps both its argv and `TDDY_SOCKET` (for asserting a resumed managed
/// session is re-wired with the orchestration prompt and per-session socket).
fn write_argv_and_env_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_argv_env.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho \"ARGV: $@\"\necho \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\n",
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
    repo: tempfile::TempDir,
    data: tempfile::TempDir,
    _stub: tempfile::TempDir,
    _cfg: tempfile::TempDir,
}

impl World {
    fn origin_main(&self) -> String {
        git(self.repo.path(), &["rev-parse", "origin/main"])
    }

    fn advance_origin_main(&self) -> String {
        git(
            self.repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "moved on"],
        );
        git(self.repo.path(), &["push", "-q", "origin", "main"]);
        self.origin_main()
    }

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

    fn metadata_of(&self, session_id: &str) -> SessionMetadata {
        read_session_metadata(&self.data.path().join("sessions").join(session_id))
            .expect("the session has metadata")
    }

    fn session_dirs(&self) -> usize {
        std::fs::read_dir(self.data.path().join("sessions"))
            .map(|d| d.count())
            .unwrap_or(0)
    }

    /// Mark a started home session stopped, as the daemon leaves it after its process exits.
    fn stop(&self, session_id: &str) {
        let dir = self.data.path().join("sessions").join(session_id);
        let mut meta = read_session_metadata(&dir).unwrap();
        meta.status = "inactive".to_string();
        meta.pid = None;
        write_session_metadata(&dir, &meta).unwrap();
    }
}

fn a_daemon_with_a_project(home_agent: Option<&str>) -> World {
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
    let (cfg, config) = write_config_with_claude_cli_binary(claude.to_str().unwrap());
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service =
        minimal_service_with_manager(config, data.path().to_path_buf(), Arc::clone(&manager));
    World {
        service,
        manager,
        repo,
        data,
        _stub: stub,
        _cfg: cfg,
    }
}

#[tokio::test]
async fn ensuring_a_project_home_starts_a_managed_session_on_a_home_worktree_at_the_main_ref() {
    // Given
    let world = a_daemon_with_a_project(Some(HOME_AGENT));

    // When
    let ensured = world.ensure().await.expect("ensure succeeds");

    // Then
    assert_eq!(ensured.outcome, "started");
    let session = ensured.session.expect("the home session is returned");
    assert_eq!(session.session_role, PROJECT_HOME_ROLE);
    assert!(session.managed_codebase, "the home session is managed");
    let meta = world.metadata_of(&session.session_id);
    assert_eq!(meta.session_role.as_deref(), Some(PROJECT_HOME_ROLE));
    let home = PathBuf::from(meta.repo_path.expect("the home session has a worktree"));
    assert_ne!(
        home,
        world.repo.path(),
        "the home worktree is not the main checkout"
    );
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), world.origin_main());
    assert_eq!(
        git(&home, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD",
        "detached"
    );
}

#[tokio::test]
async fn ensuring_again_while_it_runs_returns_the_same_session() {
    // Given
    let world = a_daemon_with_a_project(Some(HOME_AGENT));
    let first = world.ensure().await.expect("first ensure").session.unwrap();

    // When
    let again = world.ensure().await.expect("second ensure");

    // Then
    assert_eq!(again.outcome, "running");
    assert_eq!(again.session.unwrap().session_id, first.session_id);
    assert_eq!(world.session_dirs(), 1);
}

#[tokio::test]
async fn ensuring_a_stopped_home_session_fast_forwards_and_resumes_the_same_conversation() {
    // Given a home session that has stopped, and a main branch that moved on since
    let world = a_daemon_with_a_project(Some(HOME_AGENT));
    let first = world.ensure().await.expect("first ensure").session.unwrap();
    world.stop(&first.session_id);
    let moved_to = world.advance_origin_main();

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then
    assert_eq!(resumed.outcome, "resumed");
    assert_eq!(resumed.session.unwrap().session_id, first.session_id);
    let home = PathBuf::from(world.metadata_of(&first.session_id).repo_path.unwrap());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), moved_to);
    let handle = world
        .manager
        .get(&first.session_id)
        .await
        .expect("the resumed agent is running");
    let output = a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await;
    assert!(
        output.contains(&format!("--resume {}", first.session_id)),
        "the agent resumes its own conversation; got: {output:?}"
    );
}

#[tokio::test]
async fn ensuring_without_a_home_agent_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project(None);

    // When
    let refusal = world.ensure().await.expect_err("no home agent is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn ensuring_with_an_agent_that_cannot_enforce_managed_mode_is_refused() {
    // Given
    let world = a_daemon_with_a_project(Some(CURSOR_AGENT));

    // When
    let refusal = world.ensure().await.expect_err("cursor-cli is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the harness: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn a_project_home_session_is_listed_with_its_role() {
    // Given
    let world = a_daemon_with_a_project(Some(HOME_AGENT));
    let home = world.ensure().await.expect("ensure").session.unwrap();

    // When
    let listed = world
        .service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner()
        .sessions
        .into_iter()
        .find(|s| s.session_id == home.session_id)
        .expect("the home session is listed");

    // Then
    assert_eq!(listed.session_role, PROJECT_HOME_ROLE);
}
