//! Acceptance: `EnsureProjectHomeSession` sets a project's **main** (project-home session) up from
//! a New-session start spec — a managed claude-cli session on a dedicated home worktree detached at
//! the project's main branch ref — records it on the project, and afterwards reaches, resumes or
//! replaces that recorded session. Types that cannot be held read-only, sandbox placements and a
//! caller-set role are refused; a recorded session that no longer exists is cleared.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_projects::project_storage;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    AgentClonePlacement, EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse,
    ListSessionsRequest, SessionEntry, SessionService as SessionServiceTrait, SplitAgentPlacement,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const PROJECT_ID: &str = "proj-home";
const GONE_SESSION_ID: &str = "gone-0000-0000-0000-000000000000";
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
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}

/// Executable stub that dumps both its argv and `TDDY_SOCKET` (for asserting a resumed managed
/// session is re-wired with the orchestration prompt and per-session socket), then stays alive
/// like an agent waiting for input, so a started home session is still running when it is ensured
/// again.
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

/// A daemon whose data dir holds one project (`origin/main` as its main branch ref), optionally
/// recording a main session id, and a stub `claude` that echoes its argv.
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

    /// Ensure main without a start spec: reach the recorded session.
    async fn ensure(&self) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.ensure_with(None).await
    }

    /// Set main up (or replace it) from `spec`.
    async fn set_up_main(
        &self,
        spec: StartSessionRequest,
    ) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.ensure_with(Some(spec)).await
    }

    async fn ensure_with(
        &self,
        start: Option<StartSessionRequest>,
    ) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.service
            .ensure_project_home_session(Request::direct(EnsureProjectHomeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                daemon_instance_id: String::new(),
                start,
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// The session id the project records as its main, read back from `projects.yaml`.
    fn recorded_main(&self) -> Option<String> {
        project_storage::find_project(&self.data.path().join("projects"), PROJECT_ID)
            .expect("projects.yaml reads")
            .expect("the project is registered")
            .home_session_id
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

    async fn listed(&self, session_id: &str) -> SessionEntry {
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
            .expect("the session is listed")
    }

    /// Whether the daemon still serves `session_id`'s agent terminal.
    async fn is_served(&self, session_id: &str) -> bool {
        self.manager.get(session_id).await.is_some()
    }

    /// Write a harness profile named `name` running `harness` into the daemon's agents directory.
    fn with_harness_profile(&self, name: &str, harness: &str) {
        let agents = self.data.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join(format!("{name}.yaml")),
            format!("name: {name}\nharness: {harness}\nmodel: auto\n"),
        )
        .unwrap();
    }

    async fn argv_of(&self, session_id: &str) -> String {
        let handle = self
            .manager
            .get(session_id)
            .await
            .expect("the agent is running");
        a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await
    }

    /// Stop a started home session: end its agent's terminal and record it stopped, as the daemon
    /// leaves it after its process exits.
    async fn stop(&self, session_id: &str) {
        self.manager.stop_session(session_id).await;
        let dir = self.data.path().join("sessions").join(session_id);
        let mut meta = read_session_metadata(&dir).unwrap();
        meta.status = "inactive".to_string();
        meta.pid = None;
        write_session_metadata(&dir, &meta).unwrap();
    }
}

/// The New-session form's start request for a Claude CLI session on `opus`.
fn a_claude_cli_spec() -> StartSessionRequest {
    StartSessionRequest {
        session_type: "claude-cli".to_string(),
        model: "opus".to_string(),
        ..Default::default()
    }
}

fn a_daemon_with_a_project() -> World {
    a_daemon_with_a_project_recording(None)
}

fn a_daemon_with_a_project_recording(home_session_id: Option<&str>) -> World {
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
    let home_session_line = home_session_id
        .map(|id| format!("    home_session_id: {id}\n"))
        .unwrap_or_default();
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT_ID}\n    name: home\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n{home_session_line}",
            repo.path().display()
        ),
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
async fn setting_up_main_starts_a_managed_claude_cli_session_on_the_home_worktree_at_the_main_ref()
{
    // Given
    let world = a_daemon_with_a_project();

    // When
    let set_up = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("setting up main succeeds");

    // Then
    assert_eq!(set_up.outcome, "started");
    let session = set_up.session.expect("main is returned");
    assert_eq!(session.session_role, PROJECT_HOME_ROLE);
    assert!(session.managed_codebase, "main is managed");
    let meta = world.metadata_of(&session.session_id);
    assert_eq!(meta.session_role.as_deref(), Some(PROJECT_HOME_ROLE));
    let home = PathBuf::from(meta.repo_path.expect("main has a worktree"));
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
async fn setting_up_main_records_the_session_on_the_project() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let main = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("setting up main succeeds")
        .session
        .unwrap();

    // Then
    assert_eq!(world.recorded_main(), Some(main.session_id));
}

#[tokio::test]
async fn setting_up_main_runs_the_agent_with_the_specs_model_permission_mode_and_initial_prompt() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        model: "sonnet".to_string(),
        permission_mode: "plan".to_string(),
        initial_prompt: "Watch the stack".to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let main = world
        .set_up_main(spec)
        .await
        .expect("setting up main succeeds")
        .session
        .unwrap();

    // Then
    let argv = world.argv_of(&main.session_id).await;
    assert!(argv.contains("--model sonnet"), "model: {argv:?}");
    assert!(
        argv.contains("--permission-mode plan"),
        "permission mode: {argv:?}"
    );
    assert!(argv.contains("Watch the stack"), "initial prompt: {argv:?}");
}

#[tokio::test]
async fn ensuring_main_while_it_runs_returns_the_recorded_session() {
    // Given
    let world = a_daemon_with_a_project();
    let main = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();

    // When
    let again = world.ensure().await.expect("ensure reaches main");

    // Then
    assert_eq!(again.outcome, "running");
    assert_eq!(again.session.unwrap().session_id, main.session_id);
    assert_eq!(world.session_dirs(), 1);
}

#[tokio::test]
async fn ensuring_a_stopped_main_fast_forwards_and_resumes_the_same_conversation() {
    // Given main has stopped, and the main branch moved on since
    let world = a_daemon_with_a_project();
    let main = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();
    world.stop(&main.session_id).await;
    let moved_to = world.advance_origin_main();

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then
    assert_eq!(resumed.outcome, "resumed");
    assert_eq!(resumed.session.unwrap().session_id, main.session_id);
    let home = PathBuf::from(world.metadata_of(&main.session_id).repo_path.unwrap());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), moved_to);
    let argv = world.argv_of(&main.session_id).await;
    assert!(
        argv.contains(&format!("--resume {}", main.session_id)),
        "the agent resumes its own conversation; got: {argv:?}"
    );
}

#[tokio::test]
async fn ensuring_a_project_with_no_main_is_not_found_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();

    // When
    let refusal = world.ensure().await.expect_err("no main is not found");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn ensuring_a_recorded_main_that_no_longer_exists_clears_it_and_is_not_found() {
    // Given a project recording a main session that has been deleted
    let world = a_daemon_with_a_project_recording(Some(GONE_SESSION_ID));

    // When
    let refusal = world.ensure().await.expect_err("a gone main is not found");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
    assert_eq!(world.recorded_main(), None);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_as_cursor_cli_is_refused_naming_the_type_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        session_type: "cursor-cli".to_string(),
        model: "auto".to_string(),
        ..Default::default()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("cursor-cli is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the type: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_as_a_tool_session_is_refused_and_starts_nothing() {
    // Given the form's tool-session request: no session type, an agent
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        agent: "claude".to_string(),
        model: "opus".to_string(),
        ..Default::default()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a tool session is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("tool"),
        "names the type: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_in_a_sandbox_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        sandbox: true,
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a sandbox is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_with_a_requested_role_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        session_role: PROJECT_HOME_ROLE.to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a requested role is refused");

    // Then
    assert_eq!(refusal.code(), Code::PermissionDenied);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn replacing_a_running_main_stops_it_and_records_the_new_session() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();

    // When main is set up again
    let replaced = world
        .set_up_main(StartSessionRequest {
            model: "sonnet".to_string(),
            ..a_claude_cli_spec()
        })
        .await
        .expect("replacing main succeeds");

    // Then
    assert_eq!(replaced.outcome, "replaced");
    let new = replaced.session.expect("the new main is returned");
    assert_ne!(new.session_id, old.session_id);
    assert_eq!(world.recorded_main(), Some(new.session_id));
    assert!(
        world.manager.get(&old.session_id).await.is_none(),
        "the replaced main's agent is stopped"
    );
}

#[tokio::test]
async fn a_replaced_main_is_still_listed_with_its_role() {
    // Given main was replaced
    let world = a_daemon_with_a_project();
    let old = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();
    world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("replace");

    // When
    let listed = world.listed(&old.session_id).await;

    // Then it keeps its record and role, so it stays read-only
    assert_eq!(listed.session_role, PROJECT_HOME_ROLE);
}

#[tokio::test]
async fn a_project_home_session_is_listed_with_its_role() {
    // Given
    let world = a_daemon_with_a_project();
    let main = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();

    // When
    let listed = world.listed(&main.session_id).await;

    // Then
    assert_eq!(listed.session_role, PROJECT_HOME_ROLE);
}

#[tokio::test]
async fn an_ensured_main_is_pre_approved_the_project_home_tool_set() {
    // Given a main the ensure has started
    let world = a_daemon_with_a_project();
    let main = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();

    let handle = world
        .manager
        .get(&main.session_id)
        .await
        .expect("main is running");

    // Then the command line the stub `claude` echoes pre-approves the role's tool set, a
    // cross-session tool among it (the wait panics with what the agent did print)
    a_capture_showing(
        &handle.capture,
        "--allowedTools mcp__tddy-tools__ListProjectSessions",
        PTY_STUB_OUTPUT,
    )
    .await;
}

#[tokio::test]
async fn a_start_session_naming_a_role_is_refused_and_starts_nothing() {
    // Given a daemon a client can start sessions on
    let world = a_daemon_with_a_project();

    // When a client asks StartSession for the project-home role itself
    let refusal = world
        .service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_role: PROJECT_HOME_ROLE.to_string(),
            ..a_claude_cli_spec()
        }))
        .await
        .expect_err("a requested role is refused");

    // Then the role is refused as not the caller's to grant, and no session exists
    assert_eq!(refusal.code(), Code::PermissionDenied);
    assert!(
        refusal.message().contains("session_role"),
        "names the refused field: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_over_ssh_is_refused_and_starts_nothing() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        ssh_config_host: "build-box".to_string(),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("an SSH exec target is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("ssh_config_host"),
        "names the refused field: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_with_a_profile_that_runs_cursor_cli_is_refused() {
    // Given a harness profile that runs Cursor CLI, named as a claude-cli spec's agent on the
    // harness's own vendor
    let world = a_daemon_with_a_project();
    world.with_harness_profile("cursor-on-vendor", "cursor-cli");
    let spec = StartSessionRequest {
        agent: "cursor-on-vendor".to_string(),
        provider_built_in: true,
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("the profile's cursor-cli is refused");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("cursor-cli"),
        "names the type the profile resolves to: {}",
        refusal.message()
    );
    assert_eq!(world.session_dirs(), 0);
    assert_eq!(world.recorded_main(), None);
}

#[tokio::test]
async fn setting_up_main_as_an_agent_clone_is_refused() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        agent_clone: Some(AgentClonePlacement::default()),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("an agent clone is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn setting_up_main_as_a_split_agent_is_refused() {
    // Given
    let world = a_daemon_with_a_project();
    let spec = StartSessionRequest {
        split_agent: Some(SplitAgentPlacement::default()),
        ..a_claude_cli_spec()
    };

    // When
    let refusal = world
        .set_up_main(spec)
        .await
        .expect_err("a split agent is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(world.session_dirs(), 0);
}

#[tokio::test]
async fn a_refused_replace_leaves_the_running_main_running_and_recorded() {
    // Given a running main
    let world = a_daemon_with_a_project();
    let old = world
        .set_up_main(a_claude_cli_spec())
        .await
        .expect("set up")
        .session
        .unwrap();

    // When main is set up again from a spec the start refuses (claude-cli needs a model)
    let refusal = world
        .set_up_main(StartSessionRequest {
            model: String::new(),
            ..a_claude_cli_spec()
        })
        .await
        .expect_err("a start without a model is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        world.is_served(&old.session_id).await,
        "the old main still runs"
    );
    assert_eq!(world.recorded_main(), Some(old.session_id));
    assert_eq!(world.session_dirs(), 1);
}

#[tokio::test]
async fn two_concurrent_set_ups_record_one_main_and_the_other_is_replaced() {
    // Given
    let world = a_daemon_with_a_project();

    // When main is set up twice at once
    let (first, second) = tokio::join!(
        world.set_up_main(a_claude_cli_spec()),
        world.set_up_main(a_claude_cli_spec())
    );

    // Then the later one replaces the earlier, which is stopped
    let first = first.expect("the first set-up succeeds");
    let second = second.expect("the second set-up succeeds");
    assert_eq!(first.outcome, "started");
    assert_eq!(second.outcome, "replaced");
    let earlier = first.session.unwrap().session_id;
    let later = second.session.unwrap().session_id;
    assert_eq!(world.recorded_main(), Some(later));
    assert!(
        !world.is_served(&earlier).await,
        "the replaced main is no longer served"
    );
}
