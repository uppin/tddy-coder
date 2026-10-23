//! Integration tests: the `opencode-cli` and `muse-cli` session types through the daemon's session
//! host — start, resume, listing and the terminal relay — against a stub binary that records its
//! argv.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md
//! Feature: docs/ft/daemon/muse-cli-session.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    ConnectSessionRequest, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_last_appended_argv};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "fireworks/kimi-k2";
const MUSE_MODEL: &str = "muse-wonder-1.0";
const TEST_PROJECT_ID: &str = "opencode-project";
const STOPPED_SESSION_ID: &str = "019fb0c1-5a2e-7d11-9c3b-6f0a1e2d3c4b";

/// A safety net for a spawned stub to record its argv, not a prediction of how long it takes.
const SPAWN_OBSERVE: Duration = Duration::from_secs(10);

/// A daemon with one registered project whose CLI harness binary is a stub that records every
/// invocation.
struct CliHarnessDaemon {
    service: DaemonSessionHost,
    manager: Arc<CliSessionManager>,
    argv_log: PathBuf,
    sessions_base: PathBuf,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn an_opencode_daemon() -> CliHarnessDaemon {
    a_daemon_with_a_stub_for("opencode-cli", "stub_opencode.sh")
}

fn a_muse_daemon() -> CliHarnessDaemon {
    a_daemon_with_a_stub_for("muse-cli", "stub_muse.sh")
}

/// A daemon whose config names a stub, `stub_name`, as the binary of the `session_type` harness.
fn a_daemon_with_a_stub_for(session_type: &str, stub_name: &str) -> CliHarnessDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    register_project(&sessions_dir.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let argv_log = stub_dir.path().join("argv.log");
    let binary = a_stub_agent_script(stub_dir.path(), stub_name)
        .appending_argv_to(&argv_log)
        .then_reading_stdin()
        .build();

    let config = daemon_config_with_cli_harness_binary(session_type, &binary);
    let sessions_base = sessions_dir.path().to_path_buf();
    let manager = Arc::new(CliSessionManager::new());
    let service = session_host(config, sessions_base.clone(), Arc::clone(&manager));
    CliHarnessDaemon {
        service,
        manager,
        argv_log,
        sessions_base,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

impl CliHarnessDaemon {
    async fn start(&self, req: StartSessionRequest) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_session(Request::direct(req))
            .await
            .map(|resp| resp.into_inner().session_id)
    }

    /// The args of the most recent harness spawn, waiting for the stub to record them.
    async fn last_spawn_args(&self) -> Vec<String> {
        eventually("the stub harness to record a spawn", SPAWN_OBSERVE, || {
            read_last_appended_argv(&self.argv_log)
        })
        .await
    }

    async fn assert_agent_is_running(&self, session_id: &str) {
        eventually_awaiting(
            &format!("session {session_id} to register a PTY"),
            SPAWN_OBSERVE,
            || async {
                self.manager
                    .get(session_id)
                    .await
                    .map(|_| ())
                    .ok_or_else(|| "no PTY registered for this session yet".to_string())
            },
        )
        .await;
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// A stopped `session_type` session on `model` as an earlier daemon run left it: its worktree
    /// on disk (when `worktree_exists`) and a `.session.yaml` naming it.
    fn a_stopped_session(
        &self,
        session_type: &str,
        model: &str,
        session_id: &str,
        worktree_exists: bool,
    ) -> PathBuf {
        let session_dir = self.session_dir(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let worktree = self.sessions_base.join("worktrees").join(session_id);
        if worktree_exists {
            std::fs::create_dir_all(&worktree).unwrap();
        }
        let yaml = format!(
            "session_id: {session_id}\n\
             project_id: {TEST_PROJECT_ID}\n\
             created_at: 2026-10-20T05:03:46Z\n\
             updated_at: 2026-10-20T05:03:46Z\n\
             status: stopped\n\
             repo_path: {worktree}\n\
             session_type: {session_type}\n\
             model: {model}\n",
            worktree = worktree.display(),
        );
        std::fs::write(session_dir.join(".session.yaml"), yaml).unwrap();
        session_dir
    }

    async fn resume(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }
}

fn an_opencode_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "opencode-cli".to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        ..Default::default()
    }
}

fn a_muse_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "muse-cli".to_string(),
        model: MUSE_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        ..Default::default()
    }
}

fn the_value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------

#[tokio::test]
async fn starting_an_opencode_session_launches_opencode_with_the_chosen_model() {
    // Given
    let daemon = an_opencode_daemon();

    // When
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // Then the binary the daemon config names is run with the model
    daemon.assert_agent_is_running(&session_id).await;
    let argv = daemon.last_spawn_args().await;
    assert_eq!(
        the_value_after(&argv, "--model"),
        Some(TEST_MODEL),
        "{argv:?}"
    );
}

#[tokio::test]
async fn starting_an_opencode_session_opens_with_its_first_prompt() {
    // Given
    let daemon = an_opencode_daemon();
    let mut req = an_opencode_start_request();
    req.initial_prompt = "fix the flaky test".to_string();

    // When
    let session_id = daemon.start(req).await.unwrap();

    // Then
    daemon.assert_agent_is_running(&session_id).await;
    let argv = daemon.last_spawn_args().await;
    assert_eq!(
        the_value_after(&argv, "--prompt"),
        Some("fix the flaky test"),
        "{argv:?}"
    );
}

#[tokio::test]
async fn an_opencode_session_started_without_a_model_leaves_the_choice_to_opencode() {
    // Given — without an account, OpenCode uses the person's own setup and its own default model
    let daemon = an_opencode_daemon();
    let mut req = an_opencode_start_request();
    req.model = String::new();

    // When
    let session_id = daemon.start(req).await.unwrap();

    // Then
    daemon.assert_agent_is_running(&session_id).await;
    let argv = daemon.last_spawn_args().await;
    assert_eq!(the_value_after(&argv, "--model"), None, "{argv:?}");
}

#[tokio::test]
async fn an_opencode_session_records_its_type_model_and_worktree() {
    // Given
    let daemon = an_opencode_daemon();

    // When
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // Then
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).expect(".session.yaml");
    assert_eq!(meta.session_type.as_deref(), Some("opencode-cli"));
    assert_eq!(meta.model.as_deref(), Some(TEST_MODEL));
    assert!(meta.pid.is_some());
    assert!(meta
        .repo_path
        .as_ref()
        .is_some_and(|worktree| Path::new(worktree).is_dir()));
}

#[tokio::test]
async fn an_opencode_session_runs_in_its_own_worktree() {
    // Given
    let daemon = an_opencode_daemon();

    // When
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // Then the worktree is a daemon-managed one, cut for this session
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).unwrap();
    let worktree = PathBuf::from(meta.repo_path.expect("a worktree"));
    assert!(
        worktree.join(".git").exists(),
        "{} is no checkout",
        worktree.display()
    );
    assert!(worktree.components().any(|c| c.as_os_str() == ".worktrees"));
}

#[tokio::test]
async fn an_opencode_session_started_without_an_account_generates_no_config() {
    // Given
    let daemon = an_opencode_daemon();

    // When
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // Then OpenCode runs on the person's own setup
    assert!(!daemon
        .session_dir(&session_id)
        .join("opencode.json")
        .exists());
}

#[tokio::test]
async fn a_sandboxed_opencode_session_without_a_provider_account_is_refused() {
    // Given — a jail's $HOME is the session's own scratch, so the person's own OpenCode setup is
    // not there: without an account the harness would start with no credentials at all
    let daemon = an_opencode_daemon();
    let mut req = an_opencode_start_request();
    req.sandbox = true;

    // When
    let refusal = daemon
        .start(req)
        .await
        .expect_err("a jailed OpenCode needs an account");

    // Then it is refused up front, naming what it needs, and nothing is spawned
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(
        refusal
            .message
            .contains("a sandboxed opencode-cli session needs a provider account"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "opencode was spawned");
}

#[tokio::test]
async fn an_opencode_session_over_a_managed_codebase_is_refused() {
    // Given — OpenCode has no workflow of its own for a managed codebase to drive
    let daemon = an_opencode_daemon();
    let mut req = an_opencode_start_request();
    req.managed_codebase = true;

    // When
    let refusal = daemon
        .start(req)
        .await
        .expect_err("no managed OpenCode codebase");

    // Then it is refused, and nothing is spawned
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("managed_codebase"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "opencode was spawned");
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resuming_an_opencode_session_continues_the_worktrees_last_session() {
    // Given
    let daemon = an_opencode_daemon();
    daemon.a_stopped_session("opencode-cli", TEST_MODEL, STOPPED_SESSION_ID, true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then opencode is relaunched with --continue and neither a model nor a prompt
    daemon.assert_agent_is_running(STOPPED_SESSION_ID).await;
    assert_eq!(
        daemon.last_spawn_args().await,
        vec!["--continue".to_string()]
    );
}

#[tokio::test]
async fn resuming_an_opencode_session_marks_it_active_again() {
    // Given
    let daemon = an_opencode_daemon();
    let session_dir =
        daemon.a_stopped_session("opencode-cli", TEST_MODEL, STOPPED_SESSION_ID, true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.status, "active");
    assert!(meta.pid.is_some());
    assert_eq!(meta.session_type.as_deref(), Some("opencode-cli"));
}

#[tokio::test]
async fn resuming_an_opencode_session_whose_worktree_is_gone_is_refused() {
    // Given
    let daemon = an_opencode_daemon();
    daemon.a_stopped_session("opencode-cli", TEST_MODEL, STOPPED_SESSION_ID, false);

    // When
    let refusal = daemon
        .resume(STOPPED_SESSION_ID)
        .await
        .expect_err("nothing to continue");

    // Then
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(refusal.message.contains("worktree"), "{}", refusal.message);
}

// ---------------------------------------------------------------------------
// Listing and the terminal relay
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_opencode_session_is_listed_with_its_agent_and_model() {
    // Given
    let daemon = an_opencode_daemon();
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // When
    let listed = daemon
        .service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner();

    // Then
    let entry = listed
        .sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("the session is listed");
    assert_eq!(entry.session_type, "opencode-cli");
    assert_eq!(entry.agent, "opencode-cli");
    assert_eq!(entry.model, TEST_MODEL);
}

#[tokio::test]
async fn connecting_to_an_opencode_session_names_no_terminal_room() {
    // Given — its PTY is served over the daemon's own connection, never a room of its own
    let daemon = an_opencode_daemon();
    let session_id = daemon.start(an_opencode_start_request()).await.unwrap();

    // When
    let connected = daemon
        .service
        .connect_session(Request::direct(ConnectSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id,
        }))
        .await
        .expect("ConnectSession")
        .into_inner();

    // Then
    assert!(connected.livekit_room.is_empty());
    assert!(connected.livekit_url.is_empty());
    assert!(connected.livekit_server_identity.is_empty());
}

// ---------------------------------------------------------------------------
// Muse Code: the same CLI harness start and resume as OpenCode
//
// No daemon here has LiveKit configured, so a start that fell through to the tool-session path
// would fail. A start that succeeds and leaves an argv on the stub's log went through the CLI harness.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn starting_a_muse_session_launches_muse_with_the_chosen_model() {
    // Given
    let daemon = a_muse_daemon();

    // When
    let session_id = daemon.start(a_muse_start_request()).await.unwrap();

    // Then the binary the daemon config names is run with the model
    daemon.assert_agent_is_running(&session_id).await;
    let argv = daemon.last_spawn_args().await;
    assert_eq!(
        the_value_after(&argv, "--model"),
        Some(MUSE_MODEL),
        "{argv:?}"
    );
}

#[tokio::test]
async fn starting_a_muse_session_opens_with_its_first_prompt() {
    // Given
    let daemon = a_muse_daemon();
    let mut req = a_muse_start_request();
    req.initial_prompt = "fix the flaky test".to_string();

    // When
    let session_id = daemon.start(req).await.unwrap();

    // Then the prompt is Muse Code's positional argument, last, with no `--prompt` before it
    daemon.assert_agent_is_running(&session_id).await;
    assert_eq!(
        daemon.last_spawn_args().await,
        vec![
            "--model".to_string(),
            MUSE_MODEL.to_string(),
            "fix the flaky test".to_string(),
        ]
    );
}

#[tokio::test]
async fn a_muse_session_started_without_a_model_leaves_the_choice_to_muse() {
    // Given — without an account, Muse Code uses the person's own setup and its own default model
    let daemon = a_muse_daemon();
    let mut req = a_muse_start_request();
    req.model = String::new();

    // When
    let session_id = daemon.start(req).await.unwrap();

    // Then
    daemon.assert_agent_is_running(&session_id).await;
    let argv = daemon.last_spawn_args().await;
    assert_eq!(the_value_after(&argv, "--model"), None, "{argv:?}");
}

#[tokio::test]
async fn a_muse_session_records_its_type_model_and_worktree() {
    // Given
    let daemon = a_muse_daemon();

    // When
    let session_id = daemon.start(a_muse_start_request()).await.unwrap();

    // Then
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).expect(".session.yaml");
    assert_eq!(meta.session_type.as_deref(), Some("muse-cli"));
    assert_eq!(meta.model.as_deref(), Some(MUSE_MODEL));
    assert!(meta.pid.is_some());
    let worktree = PathBuf::from(meta.repo_path.expect("a worktree"));
    assert!(
        worktree.join(".git").exists(),
        "{} is no checkout",
        worktree.display()
    );
}

#[tokio::test]
async fn a_sandboxed_muse_session_without_a_provider_account_is_refused() {
    // Given — no account, so the jail would hold no Meta key for Muse Code to run on
    let daemon = a_muse_daemon();
    let mut req = a_muse_start_request();
    req.sandbox = true;

    // When
    let refusal = daemon
        .start(req)
        .await
        .expect_err("a jailed Muse Code needs an account");

    // Then it is refused up front, naming what it needs, and nothing is spawned
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(
        refusal
            .message
            .contains("a sandboxed muse-cli session needs a provider account"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "muse was spawned");
}

#[tokio::test]
async fn a_muse_session_over_a_managed_codebase_is_refused() {
    // Given — Muse Code has no workflow of its own for a managed codebase to drive
    let daemon = a_muse_daemon();
    let mut req = a_muse_start_request();
    req.managed_codebase = true;

    // When
    let refusal = daemon
        .start(req)
        .await
        .expect_err("no managed Muse Code codebase");

    // Then it is refused, and nothing is spawned
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("managed_codebase"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "muse was spawned");
}

#[tokio::test]
async fn resuming_a_muse_session_continues_its_session() {
    // Given
    let daemon = a_muse_daemon();
    daemon.a_stopped_session("muse-cli", MUSE_MODEL, STOPPED_SESSION_ID, true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then muse is relaunched with --continue and neither a model nor a prompt
    daemon.assert_agent_is_running(STOPPED_SESSION_ID).await;
    assert_eq!(
        daemon.last_spawn_args().await,
        vec!["--continue".to_string()]
    );
}

#[tokio::test]
async fn resuming_a_muse_session_marks_it_active_again() {
    // Given
    let daemon = a_muse_daemon();
    let session_dir = daemon.a_stopped_session("muse-cli", MUSE_MODEL, STOPPED_SESSION_ID, true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.status, "active");
    assert!(meta.pid.is_some());
    assert_eq!(meta.session_type.as_deref(), Some("muse-cli"));
}

#[tokio::test]
async fn resuming_a_muse_session_whose_worktree_is_gone_is_refused() {
    // Given
    let daemon = a_muse_daemon();
    daemon.a_stopped_session("muse-cli", MUSE_MODEL, STOPPED_SESSION_ID, false);

    // When
    let refusal = daemon
        .resume(STOPPED_SESSION_ID)
        .await
        .expect_err("nothing to continue");

    // Then
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(refusal.message.contains("worktree"), "{}", refusal.message);
    assert!(!daemon.argv_log.exists(), "muse was spawned");
}

#[tokio::test]
async fn a_muse_session_is_listed_with_its_agent_and_model() {
    // Given
    let daemon = a_muse_daemon();
    let session_id = daemon.start(a_muse_start_request()).await.unwrap();

    // When
    let listed = daemon
        .service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner();

    // Then
    let entry = listed
        .sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("the session is listed");
    assert_eq!(entry.session_type, "muse-cli");
    assert_eq!(entry.agent, "muse-cli");
    assert_eq!(entry.model, MUSE_MODEL);
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn daemon_config_with_cli_harness_binary(session_type: &str, binary: &Path) -> DaemonConfig {
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

fn session_host(
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
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}

fn create_test_repo_with_origin(dir: &Path) {
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

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: opencode-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}
