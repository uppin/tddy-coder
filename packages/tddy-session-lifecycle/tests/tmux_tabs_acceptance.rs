//! Acceptance tests: a session tab attached to a running tmux session — `ListTmuxSessions` lists the
//! session user's tmux sessions, a `tmux` terminal attaches a client to one, and stopping the tab
//! detaches it while the tmux session keeps running.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-attach-tab.md (`#session-tabs 3/7`)
//!
//! Each test runs a real tmux server of its own: `TMUX_TMPDIR` points tmux's socket directory at a
//! temp dir, so the daemon's tmux and the test's tmux are one private server, and no developer's
//! tmux is touched. The session's identity maps to the OS user running the test, so the tab runs as
//! that user without a privilege change. Tests mutate the process environment, so they run serially.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    ListTerminalSessionsRequest, ListTmuxSessionsRequest, StartTerminalSessionRequest,
    StopTerminalSessionRequest, TerminalSessionService as TerminalSessionServiceTrait,
    TmuxSessionInfo,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "tmux-tab-session";

/// A safety net for tmux to report a client, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

/// A private tmux server: its socket directory, and the tmux the daemon is told to run.
struct PrivateTmux {
    socket_dir: tempfile::TempDir,
}

impl PrivateTmux {
    /// Point this process — and so the daemon's tmux — at a fresh socket dir, with tmux on PATH.
    fn started() -> Self {
        let socket_dir = tempfile::tempdir().unwrap();
        std::env::set_var("TMUX_TMPDIR", socket_dir.path());
        std::env::remove_var("TDDY_TMUX_BINARY");
        PrivateTmux { socket_dir }
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .env("TMUX_TMPDIR", self.socket_dir.path())
            .env_remove("TMUX")
            .args(args)
            .output()
            .expect("tmux runs")
    }

    /// Start a detached tmux session `name` with `windows` windows.
    fn with_session(self, name: &str, windows: usize) -> Self {
        assert!(self
            .tmux(&["new-session", "-d", "-s", name])
            .status
            .success());
        for _ in 1..windows {
            assert!(self.tmux(&["new-window", "-t", name]).status.success());
        }
        self
    }

    fn attached_clients(&self, name: &str) -> usize {
        let out = self.tmux(&["list-clients", "-t", name]);
        String::from_utf8_lossy(&out.stdout).lines().count()
    }

    fn has_session(&self, name: &str) -> bool {
        self.tmux(&["has-session", "-t", name]).status.success()
    }

    fn pane_text(&self, name: &str) -> String {
        String::from_utf8_lossy(&self.tmux(&["capture-pane", "-p", "-t", name]).stdout).into_owned()
    }
}

impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
        std::env::remove_var("TMUX_TMPDIR");
    }
}

/// A daemon serving one running session whose identity maps to the OS user running the test.
struct TmuxTabDaemon {
    service: tddy_terminal_rpc::TerminalSessionServiceImpl,
    manager: Arc<ClaudeCliSessionManager>,
    _dirs: Vec<tempfile::TempDir>,
}

async fn a_daemon_with_a_running_session() -> TmuxTabDaemon {
    let (config_dir, config) = a_config_mapping_to_this_user();
    let worktree = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ClaudeCliSessionManager::new());
    start_main_terminal(&manager, worktree.path()).await;
    let service = session_host(config, data_dir.path().to_path_buf(), Arc::clone(&manager))
        .terminal_session_service();
    TmuxTabDaemon {
        service,
        manager,
        _dirs: vec![config_dir, worktree, data_dir],
    }
}

impl TmuxTabDaemon {
    async fn list_tmux_sessions(&self) -> Result<Vec<TmuxSessionInfo>, tddy_rpc::Status> {
        self.service
            .list_tmux_sessions(Request::direct(ListTmuxSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().sessions)
    }

    async fn start_tmux_tab(&self, tmux_session: &str) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                control_token: String::new(),
                kind: "tmux".to_string(),
                tmux_session: tmux_session.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    async fn stop_tab(&self, terminal_id: &str) {
        self.service
            .stop_terminal_session(Request::direct(StopTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("StopTerminalSession must succeed");
    }

    async fn type_into(&self, terminal_id: &str, text: &str) {
        let tab = self
            .manager
            .get_terminal(SESSION_ID, terminal_id)
            .await
            .expect("the tab is running");
        tab.stdin_tx.send(Bytes::from(text.to_string())).unwrap();
    }
}

fn names(sessions: &[TmuxSessionInfo]) -> Vec<(&str, u32, u32)> {
    sessions
        .iter()
        .map(|s| (s.name.as_str(), s.windows, s.attached))
        .collect()
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_returns_the_users_sessions_with_window_and_attached_counts() {
    // Given
    let _tmux = PrivateTmux::started()
        .with_session("alpha", 2)
        .with_session("beta", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let sessions = daemon
        .list_tmux_sessions()
        .await
        .expect("tmux sessions listed");

    // Then
    assert_eq!(names(&sessions), vec![("alpha", 2, 0), ("beta", 1, 0)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_without_a_tmux_server_is_empty() {
    // Given no tmux server on the private socket
    let _tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let sessions = daemon
        .list_tmux_sessions()
        .await
        .expect("an empty listing, not an error");

    // Then
    assert!(sessions.is_empty(), "{sessions:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_without_tmux_installed_is_refused_naming_tmux() {
    // Given a tmux binary that does not exist
    let _tmux = PrivateTmux::started();
    std::env::set_var("TDDY_TMUX_BINARY", "/nonexistent/tmux");
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let refusal = daemon.list_tmux_sessions().await.unwrap_err();
    std::env::remove_var("TDDY_TMUX_BINARY");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("tmux"), "{}", refusal.message());
}

// ---------------------------------------------------------------------------
// A tmux tab
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_tab_attaches_to_the_named_session_and_carries_input_and_output() {
    // Given
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When a tab is attached and the operator types into it
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");
    eventually("the tab's client to attach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 1)
            .then_some(())
            .ok_or_else(|| "no client attached yet".to_string())
    })
    .await;
    daemon.type_into(&tab, "echo typed-into-the-tab\r").await;

    // Then the keystrokes reach the tmux pane
    eventually("the pane to show the typed command", OBSERVE, || {
        let text = tmux.pane_text("alpha");
        text.contains("typed-into-the-tab")
            .then_some(())
            .ok_or_else(|| format!("pane holds {text:?}"))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn stopping_a_tmux_tab_detaches_and_leaves_the_tmux_session_running() {
    // Given an attached tab
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");
    eventually("the tab's client to attach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 1)
            .then_some(())
            .ok_or_else(|| "no client attached yet".to_string())
    })
    .await;

    // When the tab is closed
    daemon.stop_tab(&tab).await;

    // Then its client is gone and the tmux session is still there
    eventually("the tab's client to detach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 0)
            .then_some(())
            .ok_or_else(|| "a client is still attached".to_string())
    })
    .await;
    assert!(tmux.has_session("alpha"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_tab_for_an_unknown_session_is_not_found() {
    // Given
    let _tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let refusal = daemon.start_tmux_tab("never-started").await.unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_terminal_sessions_reports_a_tmux_tab_with_its_tmux_session_name() {
    // Given an attached tab
    let _tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");

    // When
    let terminals = daemon
        .service
        .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("ListTerminalSessions must succeed")
        .into_inner()
        .terminals;

    // Then
    let listed = terminals
        .iter()
        .find(|t| t.terminal_id == tab)
        .expect("the tmux tab is listed");
    assert_eq!(listed.kind, "tmux");
    assert_eq!(listed.tmux_session, "alpha");
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn this_os_user() -> String {
    std::env::var("USER").expect("USER names the OS user running the test")
}

fn a_config_mapping_to_this_user() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(
        &path,
        format!(
            "users:\n  - github_user: \"testuser\"\n    os_user: \"{}\"\n",
            this_os_user()
        ),
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

async fn start_main_terminal(manager: &ClaudeCliSessionManager, worktree: &Path) {
    let main = a_stub_agent_script(worktree, "stub_main.sh")
        .then_reading_stdin()
        .build();
    manager
        .start(
            SESSION_ID,
            worktree.to_path_buf(),
            "claude-opus-4-8",
            main.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("main terminal must start");
}

fn session_host(
    config: DaemonConfig,
    data_dir: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> DaemonSessionHost {
    let sessions_base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}
