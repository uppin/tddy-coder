//! Acceptance tests: a session tab on a new tmux session — a `tmux-new` terminal creates the named
//! tmux session in the session's worktree (or attaches when it exists) and stopping the tab detaches
//! it while the tmux session keeps running.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-new-session-tab.md (`#session-tabs 7/7`)
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
    ListTmuxSessionsRequest, StartTerminalSessionRequest, StopTerminalSessionRequest,
    TerminalSessionService as TerminalSessionServiceTrait, TmuxSessionInfo,
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

    /// The working directory of `name`'s first pane.
    fn pane_directory(&self, name: &str) -> PathBuf {
        let out = self.tmux(&["display-message", "-p", "-t", name, "#{pane_current_path}"]);
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
            .canonicalize()
            .unwrap()
    }

    #[allow(dead_code)]
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
    worktree: PathBuf,
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
        worktree: worktree.path().canonicalize().unwrap(),
        _dirs: vec![config_dir, worktree, data_dir],
    }
}

impl TmuxTabDaemon {
    #[allow(dead_code)]
    async fn list_tmux_sessions(&self) -> Result<Vec<TmuxSessionInfo>, tddy_rpc::Status> {
        self.service
            .list_tmux_sessions(Request::direct(ListTmuxSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().sessions)
    }

    async fn start_new_tmux_tab(&self, name: &str) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                control_token: String::new(),
                kind: "tmux-new".to_string(),
                tmux_session: name.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    async fn type_into(&self, terminal_id: &str, text: &str) {
        let tab = self
            .manager
            .get_terminal(SESSION_ID, terminal_id)
            .await
            .expect("the tab is running");
        tab.stdin_tx.send(Bytes::from(text.to_string())).unwrap();
    }
}

async fn until_attached(tmux: &PrivateTmux, name: &str, clients: usize) {
    eventually("the tab's client count to settle", OBSERVE, || {
        let attached = tmux.attached_clients(name);
        (attached == clients)
            .then_some(())
            .ok_or_else(|| format!("{attached} clients attached, waiting for {clients}"))
    })
    .await;
}

// ---------------------------------------------------------------------------
// A tab on a new tmux session
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_creates_the_named_session_in_the_worktree_and_attaches() {
    // Given no tmux session named work
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon
        .start_new_tmux_tab("work")
        .await
        .expect("a new tmux session tab starts");

    // Then tmux runs work, rooted in the session's worktree, with the tab attached
    until_attached(&tmux, "work", 1).await;
    assert_eq!(tmux.pane_directory("work"), daemon.worktree);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn stopping_a_tmux_new_tab_leaves_its_session_running() {
    // Given a tab on a new session
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_new_tmux_tab("work")
        .await
        .expect("a new tmux session tab starts");
    until_attached(&tmux, "work", 1).await;

    // When
    daemon.stop_tab(&tab).await;

    // Then
    until_attached(&tmux, "work", 0).await;
    assert!(tmux.has_session("work"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_for_an_existing_name_attaches_to_it() {
    // Given a session named work that already has two windows
    let tmux = PrivateTmux::started().with_session("work", 2);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon
        .start_new_tmux_tab("work")
        .await
        .expect("the tab attaches");

    // Then the existing session gained a client and was not replaced
    until_attached(&tmux, "work", 1).await;
    let windows = tmux.tmux(&["list-windows", "-t", "work"]);
    assert_eq!(String::from_utf8_lossy(&windows.stdout).lines().count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_with_an_empty_name_is_refused() {
    let _tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    let refusal = daemon.start_new_tmux_tab("  ").await.unwrap_err();

    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_with_a_colon_in_its_name_is_refused() {
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    let refusal = daemon.start_new_tmux_tab("a:b").await.unwrap_err();

    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(!tmux.has_session("a"));
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
