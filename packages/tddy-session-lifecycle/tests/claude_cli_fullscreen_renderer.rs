//! Integration: every `claude-cli` launch — a fresh start and a Resume, on the host and in the
//! sandbox — runs Claude's fullscreen renderer unless the operator turns it off with
//! `claude_cli.fullscreen: false`.
//!
//! Only the fullscreen renderer enables the alternate screen and mouse tracking (DECSET 1049 /
//! 1000 / 1006). Without them the web terminal has nothing to route the wheel to: its live pane
//! keeps no scrollback, so an inline Claude cannot be scrolled at all. Claude reads the choice from
//! `CLAUDE_CODE_NO_FLICKER=1`, so the stub agent here reports what it was launched with — whether
//! it is a Resume (`--resume` in its argv) and that variable — on one line.
//!
//! A host agent inherits the daemon's own environment, so the `fullscreen: false` cases assume the
//! test runner does not itself export `CLAUDE_CODE_NO_FLICKER`.
//!
//! The sandboxed cases need Seatbelt and run on macOS only, like the rest of the sandbox suites.
#![allow(dead_code, unused_imports)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "fullscreen-renderer-project";
const RESUMED_SESSION_ID: &str = "01900000-0000-7000-8000-0000000f5c01";

/// What the stub prints when a fresh start runs the fullscreen renderer.
const FRESH_START_IN_FULLSCREEN: &str = "LAUNCH resume=no CLAUDE_CODE_NO_FLICKER=1";
/// What the stub prints when a fresh start runs the classic renderer.
const FRESH_START_INLINE: &str = "LAUNCH resume=no CLAUDE_CODE_NO_FLICKER=unset";
/// What the stub prints when a Resume runs the fullscreen renderer.
const RESUME_IN_FULLSCREEN: &str = "LAUNCH resume=yes CLAUDE_CODE_NO_FLICKER=1";
/// What the stub prints when a Resume runs the classic renderer.
const RESUME_INLINE: &str = "LAUNCH resume=yes CLAUDE_CODE_NO_FLICKER=unset";

/// The jailed runner boots a Seatbelt profile before the agent prints anything.
const SANDBOXED_STUB_OUTPUT: Duration = Duration::from_secs(30);

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The OS user the test process runs as — a real, resolvable user, so the spawn needs no
/// privilege drop.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// How the daemon config states the renderer choice.
enum Renderer {
    /// `claude_cli:` does not mention `fullscreen` — the default.
    Unstated,
    /// `claude_cli.fullscreen: false`.
    TurnedOff,
}

/// A daemon config whose `claude_cli` is the stub agent, stating the renderer as given.
fn a_daemon_config(stub_agent: &Path, renderer: Renderer) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let fullscreen_line = match renderer {
        Renderer::Unstated => "",
        Renderer::TurnedOff => "  fullscreen: false\n",
    };
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nallowed_tools:\n  - path: /bin/true\n    label: true\nclaude_cli:\n  binary_path: {}\n{fullscreen_line}",
        stub_agent.display()
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

/// A stub `claude` that reports how it was launched on one line, then stays alive on stdin.
fn a_stub_agent_reporting_its_launch(dir: &Path) -> PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(
        &script_path,
        r#"#!/bin/sh
case " $* " in
  *" --resume "*) resumed=yes ;;
  *) resumed=no ;;
esac
echo "LAUNCH resume=$resumed CLAUDE_CODE_NO_FLICKER=${CLAUDE_CODE_NO_FLICKER:-unset}"
cat
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

fn a_session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> Arc<DaemonSessionHost> {
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
    let host = Arc::new(DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    ));
    // A sandboxed launch reaches the jail over this bridge; the daemon runtime installs it at mount.
    host.install_sandbox_rpc_bridge();
    host
}

/// A git repo with an `origin`, registered as the test project under `sessions_base/projects`.
fn a_registered_project(sessions_base: &Path) -> tempfile::TempDir {
    let repo_dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo_dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", repo_dir.path().to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);

    let projects_dir = sessions_base.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: {TEST_PROJECT_ID}\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo_dir.path().display()
        ),
    )
    .unwrap();
    repo_dir
}

fn a_claude_cli_start_request(sandbox: bool) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        sandbox,
        ..Default::default()
    }
}

/// An inactive host `claude-cli` session whose process has exited, ready to be resumed.
fn an_exited_host_session(sessions_base: &Path, worktree: &Path) {
    let session_dir = sessions_base.join("sessions").join(RESUMED_SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: RESUMED_SESSION_ID.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-10-24T08:00:00Z".to_string(),
        updated_at: "2026-10-24T08:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree.to_str().unwrap().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
    };
    write_session_metadata(&session_dir, &meta).unwrap();
}

// ---------------------------------------------------------------------------
// Drivers
// ---------------------------------------------------------------------------

/// What a host session's agent printed, once it shows `expected`.
async fn host_agent_output_showing(
    manager: &ClaudeCliSessionManager,
    session_id: &str,
    expected: &str,
) -> String {
    let handle = manager
        .get(session_id)
        .await
        .expect("the session must be registered in the shared manager");
    a_capture_showing(&handle.capture, expected, PTY_STUB_OUTPUT).await
}

/// What a session's terminal stream showed — up to `expected`, or everything it showed within the
/// sandbox boot budget when `expected` never arrived.
async fn terminal_stream_output_until(
    host: &DaemonSessionHost,
    session_id: &str,
    expected: &str,
) -> String {
    let mut stream = host
        .terminal_session_service()
        .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
            terminal_id: String::new(),
            initial_cols: 80,
            initial_rows: 24,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            control_token: String::new(),
        }))
        .await
        .expect("stream_terminal_output must succeed")
        .into_inner();
    let mut shown = String::new();
    let _ = tokio::time::timeout(SANDBOXED_STUB_OUTPUT, async {
        while let Some(Ok(msg)) = stream.next().await {
            shown.push_str(&String::from_utf8_lossy(&msg.data));
            if shown.contains(expected) {
                return;
            }
        }
    })
    .await;
    shown
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_host_session_starts_claude_in_fullscreen_by_default() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = a_stub_agent_reporting_its_launch(stub_dir.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::Unstated);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    let session_id = host
        .start_session(Request::direct(a_claude_cli_start_request(false)))
        .await
        .expect("StartSession must succeed")
        .into_inner()
        .session_id;

    // Then
    host_agent_output_showing(&manager, &session_id, FRESH_START_IN_FULLSCREEN).await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_host_session_starts_claude_inline_when_fullscreen_is_turned_off() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let _repo = a_registered_project(sessions_tmp.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = a_stub_agent_reporting_its_launch(stub_dir.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::TurnedOff);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    let session_id = host
        .start_session(Request::direct(a_claude_cli_start_request(false)))
        .await
        .expect("StartSession must succeed")
        .into_inner()
        .session_id;

    // Then
    host_agent_output_showing(&manager, &session_id, FRESH_START_INLINE).await;
}

#[tokio::test]
#[serial_test::serial]
async fn resuming_a_host_session_relaunches_claude_in_fullscreen_by_default() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    an_exited_host_session(sessions_tmp.path(), worktree.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = a_stub_agent_reporting_its_launch(stub_dir.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::Unstated);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    host.resume_session(Request::direct(ResumeSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: RESUMED_SESSION_ID.to_string(),
    }))
    .await
    .expect("ResumeSession must succeed for an exited claude-cli session");

    // Then
    host_agent_output_showing(&manager, RESUMED_SESSION_ID, RESUME_IN_FULLSCREEN).await;
}

#[tokio::test]
#[serial_test::serial]
async fn resuming_a_host_session_relaunches_claude_inline_when_fullscreen_is_turned_off() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    an_exited_host_session(sessions_tmp.path(), worktree.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = a_stub_agent_reporting_its_launch(stub_dir.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::TurnedOff);
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    host.resume_session(Request::direct(ResumeSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: RESUMED_SESSION_ID.to_string(),
    }))
    .await
    .expect("ResumeSession must succeed for an exited claude-cli session");

    // Then
    host_agent_output_showing(&manager, RESUMED_SESSION_ID, RESUME_INLINE).await;
}

// ---------------------------------------------------------------------------
// Sandbox — the jail's environment is constructed, never inherited from the daemon
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[tokio::test]
#[serial_test::serial]
async fn a_sandboxed_session_starts_claude_in_fullscreen_by_default() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo = a_registered_project(sessions_tmp.path());
    let stub = a_stub_agent_reporting_its_launch(repo.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::Unstated);
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    let session_id = host
        .start_session(Request::direct(a_claude_cli_start_request(true)))
        .await
        .expect("sandboxed StartSession must succeed on darwin")
        .into_inner()
        .session_id;

    // Then
    let shown = terminal_stream_output_until(&host, &session_id, FRESH_START_IN_FULLSCREEN).await;
    assert!(
        shown.contains(FRESH_START_IN_FULLSCREEN),
        "the jailed agent must be launched with CLAUDE_CODE_NO_FLICKER=1; it showed {shown:?}"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[serial_test::serial]
async fn a_sandboxed_session_starts_claude_inline_when_fullscreen_is_turned_off() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo = a_registered_project(sessions_tmp.path());
    let stub = a_stub_agent_reporting_its_launch(repo.path());
    let (_cfg, config) = a_daemon_config(&stub, Renderer::TurnedOff);
    let host = a_session_host(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    let session_id = host
        .start_session(Request::direct(a_claude_cli_start_request(true)))
        .await
        .expect("sandboxed StartSession must succeed on darwin")
        .into_inner()
        .session_id;

    // Then
    let shown = terminal_stream_output_until(&host, &session_id, FRESH_START_INLINE).await;
    assert!(
        shown.contains(FRESH_START_INLINE),
        "the jailed agent must be launched without CLAUDE_CODE_NO_FLICKER; it showed {shown:?}"
    );
}

/// The session is started by a daemon with fullscreen turned off and resumed by one that leaves
/// it at the default, so only the relaunched agent can report the fullscreen renderer.
#[cfg(target_os = "macos")]
#[tokio::test]
#[serial_test::serial]
async fn resuming_a_sandboxed_session_relaunches_claude_in_fullscreen_by_default() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo = a_registered_project(sessions_tmp.path());
    let stub = a_stub_agent_reporting_its_launch(repo.path());
    let (_off_cfg, inline_config) = a_daemon_config(&stub, Renderer::TurnedOff);
    let starting_host = a_session_host(
        inline_config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );
    let session_id = starting_host
        .start_session(Request::direct(a_claude_cli_start_request(true)))
        .await
        .expect("sandboxed StartSession must succeed on darwin")
        .into_inner()
        .session_id;
    let (_default_cfg, default_config) = a_daemon_config(&stub, Renderer::Unstated);
    let resuming_host = a_session_host(
        default_config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    resuming_host
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession must succeed for a sandboxed claude-cli session");

    // Then
    let shown =
        terminal_stream_output_until(&resuming_host, &session_id, "CLAUDE_CODE_NO_FLICKER=1").await;
    assert!(
        shown.contains("CLAUDE_CODE_NO_FLICKER=1"),
        "the relaunched jailed agent must be launched with CLAUDE_CODE_NO_FLICKER=1; it showed {shown:?}"
    );
}
