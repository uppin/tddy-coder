//! Integration tests: the `warp-cli` session type through the daemon's session host — start,
//! the resume token Warp prints as it exits, resume into that conversation, refusals and listing —
//! against a stub `warp` that records its argv and prints Warp's exit banner.
//!
//! Feature: docs/ft/daemon/warp-cli-session.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    ListSessionsRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_last_appended_argv};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_PROJECT_ID: &str = "warp-project";
const STOPPED_SESSION_ID: &str = "019fb0c1-5a2e-7d11-9c3b-6f0a1e2d3c4b";

/// A safety net for a spawned stub to record its argv or for a recorded token to land, not a
/// prediction of how long either takes.
const SPAWN_OBSERVE: Duration = Duration::from_secs(10);

/// How the stub `warp` ends each run.
enum OnExit {
    /// Prints Warp's banner naming `tok-<n>`, where `n` counts this stub's runs from 1.
    PrintsANewTokenEachRun,
    /// Prints nothing — a run that ended before Warp could say how to resume it.
    PrintsNoToken,
}

/// A daemon with one registered project whose `warp-cli` binary is a stub that records every
/// invocation and then exits as `on_exit` says.
struct WarpDaemon {
    service: DaemonSessionHost,
    manager: Arc<CliSessionManager>,
    argv_log: PathBuf,
    sessions_base: PathBuf,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_warp_daemon(on_exit: OnExit) -> WarpDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    register_project(&sessions_dir.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let argv_log = stub_dir.path().join("argv.log");
    let stub = a_stub_agent_script(stub_dir.path(), "stub_warp.sh").appending_argv_to(&argv_log);
    let binary = match on_exit {
        OnExit::PrintsANewTokenEachRun => stub.then_running(&warps_exit_banner(&argv_log)),
        OnExit::PrintsNoToken => stub,
    }
    .build();

    let config = daemon_config_with_cli_harness_binary("warp-cli", &binary);
    let sessions_base = sessions_dir.path().to_path_buf();
    let manager = Arc::new(CliSessionManager::new());
    let service = session_host(config, sessions_base.clone(), Arc::clone(&manager));
    WarpDaemon {
        service,
        manager,
        argv_log,
        sessions_base,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

/// What Warp prints as it exits — dimmed prose, then the command in bold, CRLF line ends — naming
/// the token of this run, counted from the stub's own invocation log.
fn warps_exit_banner(argv_log: &Path) -> String {
    format!(
        r#"TDDY_WARP_RUN=$(wc -l < "{log}" | tr -d ' ')
printf '\033[2mResume this conversation with:\033[0m\r\n  \033[1mwarp --resume tok-%s\033[0m\r\n' "$TDDY_WARP_RUN""#,
        log = argv_log.display()
    )
}

impl WarpDaemon {
    async fn start(&self, req: StartSessionRequest) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_session(Request::direct(req))
            .await
            .map(|resp| resp.into_inner().session_id)
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

    /// The args of the most recent `warp` spawn, waiting for the stub to record them.
    async fn last_spawn_args(&self) -> Vec<String> {
        eventually("the stub warp to record a spawn", SPAWN_OBSERVE, || {
            read_last_appended_argv(&self.argv_log)
        })
        .await
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// The resume token `.session.yaml` holds, waiting for it to read `expected`.
    async fn recorded_token_becomes(&self, session_id: &str, expected: &str) {
        let session_dir = self.session_dir(session_id);
        eventually(
            &format!("session {session_id} to record resume token {expected:?}"),
            SPAWN_OBSERVE,
            || {
                let meta = read_session_metadata(&session_dir).map_err(|e| e.to_string())?;
                match meta.harness_resume_token.as_deref() {
                    Some(token) if token == expected => Ok(()),
                    other => Err(format!("recorded token is {other:?}")),
                }
            },
        )
        .await;
    }

    /// Waits until the session's `warp` has exited and its terminal is deregistered.
    async fn warp_has_exited(&self, session_id: &str) {
        eventually_awaiting(
            &format!("session {session_id}'s warp to exit"),
            SPAWN_OBSERVE,
            || async {
                match self.manager.get(session_id).await {
                    None => Ok(()),
                    Some(_) => Err("the PTY is still registered".to_string()),
                }
            },
        )
        .await;
    }

    /// A stopped `warp-cli` session as an earlier daemon run left it: its worktree on disk (when
    /// `worktree_exists`) and a `.session.yaml` naming it, with `token` recorded when given.
    fn a_stopped_warp_session(&self, token: Option<&str>, worktree_exists: bool) -> PathBuf {
        let session_dir = self.session_dir(STOPPED_SESSION_ID);
        std::fs::create_dir_all(&session_dir).unwrap();
        let worktree = self
            .sessions_base
            .join("worktrees")
            .join(STOPPED_SESSION_ID);
        if worktree_exists {
            std::fs::create_dir_all(&worktree).unwrap();
        }
        let token_line = token
            .map(|token| format!("harness_resume_token: {token}\n"))
            .unwrap_or_default();
        let yaml = format!(
            "session_id: {STOPPED_SESSION_ID}\n\
             project_id: {TEST_PROJECT_ID}\n\
             created_at: 2026-10-20T05:03:46Z\n\
             updated_at: 2026-10-20T05:03:46Z\n\
             status: stopped\n\
             repo_path: {worktree}\n\
             session_type: warp-cli\n\
             {token_line}",
            worktree = worktree.display(),
        );
        std::fs::write(session_dir.join(".session.yaml"), yaml).unwrap();
        session_dir
    }
}

fn a_warp_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "warp-cli".to_string(),
        model: String::new(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------

#[tokio::test]
async fn starting_a_warp_session_launches_warp_with_no_arguments() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);

    // When
    daemon.start(a_warp_start_request()).await.unwrap();

    // Then
    assert_eq!(daemon.last_spawn_args().await, Vec::<String>::new());
}

#[tokio::test]
async fn a_warp_session_records_its_type_and_worktree_and_no_model() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);

    // When
    let session_id = daemon.start(a_warp_start_request()).await.unwrap();

    // Then
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).unwrap();
    assert_eq!(meta.session_type.as_deref(), Some("warp-cli"));
    assert_eq!(meta.model, None);
    assert!(meta.pid.is_some());
    let worktree = PathBuf::from(meta.repo_path.expect("the worktree is recorded"));
    assert!(worktree.exists(), "{} was not cut", worktree.display());
}

#[tokio::test]
async fn a_warp_session_runs_in_its_own_worktree_on_a_warp_cli_branch() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);

    // When
    let session_id = daemon.start(a_warp_start_request()).await.unwrap();

    // Then
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).unwrap();
    let worktree = PathBuf::from(meta.repo_path.expect("a worktree"));
    assert!(worktree.components().any(|c| c.as_os_str() == ".worktrees"));
    assert!(
        the_branch_checked_out_in(&worktree).starts_with("warp-cli/"),
        "{}",
        the_branch_checked_out_in(&worktree)
    );
}

// ---------------------------------------------------------------------------
// The resume token Warp prints as it exits
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_warp_session_records_the_resume_token_warp_prints_on_exit() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsANewTokenEachRun);

    // When
    let session_id = daemon.start(a_warp_start_request()).await.unwrap();

    // Then
    daemon.recorded_token_becomes(&session_id, "tok-1").await;
}

#[tokio::test]
async fn a_warp_session_that_is_seen_to_have_exited_already_records_its_resume_token() {
    // Given: a stub that prints its banner and exits at once
    let daemon = a_warp_daemon(OnExit::PrintsANewTokenEachRun);
    let session_id = daemon.start(a_warp_start_request()).await.unwrap();

    // When: the session is first seen to have exited (its terminal gone)
    daemon.warp_has_exited(&session_id).await;

    // Then: the token is on disk already, with no further waiting, so a resume now continues it
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).unwrap();
    assert_eq!(meta.harness_resume_token.as_deref(), Some("tok-1"));
}

#[tokio::test]
async fn a_later_exit_replaces_the_recorded_resume_token() {
    // Given: a stopped session whose earlier run recorded a token
    let daemon = a_warp_daemon(OnExit::PrintsANewTokenEachRun);
    daemon.a_stopped_warp_session(Some("tok-earlier"), true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then: the resumed run is the stub's first, and its token replaces the earlier one
    daemon
        .recorded_token_becomes(STOPPED_SESSION_ID, "tok-1")
        .await;
}

#[tokio::test]
async fn an_exit_that_prints_no_token_keeps_the_recorded_one() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let session_dir = daemon.a_stopped_warp_session(Some("tok-earlier"), true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();
    daemon.warp_has_exited(STOPPED_SESSION_ID).await;

    // Then
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.harness_resume_token.as_deref(), Some("tok-earlier"));
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resuming_a_warp_session_continues_its_recorded_conversation() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    daemon.a_stopped_warp_session(Some("tok-123"), true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then
    assert_eq!(
        daemon.last_spawn_args().await,
        vec!["--resume".to_string(), "tok-123".to_string()]
    );
}

#[tokio::test]
async fn resuming_a_warp_session_with_no_recorded_token_starts_fresh() {
    // Given: a session whose warp never exited cleanly, so it never printed a token
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    daemon.a_stopped_warp_session(None, true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then: a fresh conversation in the same worktree, never a bare `--resume`
    assert_eq!(daemon.last_spawn_args().await, Vec::<String>::new());
}

#[tokio::test]
async fn resuming_a_warp_session_marks_it_active_again() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let session_dir = daemon.a_stopped_warp_session(Some("tok-123"), true);

    // When
    daemon.resume(STOPPED_SESSION_ID).await.unwrap();

    // Then
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.status, "active");
    assert!(meta.pid.is_some());
    assert_eq!(meta.session_type.as_deref(), Some("warp-cli"));
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_sandboxed_warp_session_is_refused() {
    // Given — the sandbox runner hosts no Warp
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let mut req = a_warp_start_request();
    req.sandbox = true;

    // When
    let refusal = daemon.start(req).await.expect_err("no jailed Warp");

    // Then it is refused up front, naming the harness, and nothing is spawned
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(
        refusal.message.contains("does not host warp-cli"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "warp was spawned");
}

#[tokio::test]
async fn a_warp_session_over_a_managed_codebase_is_refused() {
    // Given — Warp has no workflow of its own for a managed codebase to drive
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let mut req = a_warp_start_request();
    req.managed_codebase = true;

    // When
    let refusal = daemon
        .start(req)
        .await
        .expect_err("no managed Warp codebase");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("managed_codebase"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "warp was spawned");
}

#[tokio::test]
async fn a_warp_session_on_a_provider_account_is_refused() {
    // Given — Warp runs on the person's own Warp sign-in
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let mut req = a_warp_start_request();
    req.provider_account_id = "meta-hackathon".to_string();

    // When
    let refusal = daemon.start(req).await.expect_err("Warp takes no account");

    // Then it is refused naming Warp, before anything is cut or spawned
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal
            .message
            .contains("Warp cannot be pointed at another provider"),
        "{}",
        refusal.message
    );
    assert!(!daemon.argv_log.exists(), "warp was spawned");
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_warp_session_is_listed_with_its_agent() {
    // Given
    let daemon = a_warp_daemon(OnExit::PrintsNoToken);
    let session_id = daemon.start(a_warp_start_request()).await.unwrap();

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
    assert_eq!(entry.session_type, "warp-cli");
    assert_eq!(entry.agent, "warp-cli");
    assert_eq!(entry.model, "");
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

fn the_branch_checked_out_in(worktree: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree)
        .output()
        .expect("git rev-parse");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: warp-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}
