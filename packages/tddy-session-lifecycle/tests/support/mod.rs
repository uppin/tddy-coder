//! The fixtures the tmux session-tab suites (`tmux_tabs_acceptance`, `tmux_new_session_acceptance`)
//! share: a private tmux server, and a daemon serving one running session whose identity maps to the
//! OS user running the test.
//!
//! Each suite uses a subset, so unused-item warnings are allowed for the module.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    ListTmuxSessionsRequest, StartTerminalSessionRequest, StopTerminalSessionRequest,
    TerminalSessionService as TerminalSessionServiceTrait, TmuxSessionInfo,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub const VALID_TOKEN: &str = "valid-token";
pub const SESSION_ID: &str = "tmux-tab-session";

/// A safety net for tmux to report a client, not a prediction of how long it takes.
pub const OBSERVE: Duration = Duration::from_secs(10);

/// A private tmux server: its socket directory, and the tmux the daemon is told to run.
pub struct PrivateTmux {
    socket_dir: tempfile::TempDir,
}

impl PrivateTmux {
    /// Point this process — and so the daemon's tmux — at a fresh socket dir, with tmux on PATH.
    pub fn started() -> Self {
        let socket_dir = tempfile::tempdir().unwrap();
        std::env::set_var("TMUX_TMPDIR", socket_dir.path());
        std::env::remove_var("TDDY_TMUX_BINARY");
        PrivateTmux { socket_dir }
    }

    pub fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .env("TMUX_TMPDIR", self.socket_dir.path())
            .env_remove("TMUX")
            .args(args)
            .output()
            .expect("tmux runs")
    }

    /// Tell the daemon to run `binary` as tmux — cleared again when this server is dropped.
    pub fn with_tmux_binary(self, binary: &str) -> Self {
        std::env::set_var("TDDY_TMUX_BINARY", binary);
        self
    }

    /// Start a detached tmux session `name` with `windows` windows.
    pub fn with_session(self, name: &str, windows: usize) -> Self {
        assert!(self
            .tmux(&["new-session", "-d", "-s", name])
            .status
            .success());
        for _ in 1..windows {
            assert!(self.tmux(&["new-window", "-t", name]).status.success());
        }
        self
    }

    pub fn attached_clients(&self, name: &str) -> usize {
        let out = self.tmux(&["list-clients", "-t", name]);
        String::from_utf8_lossy(&out.stdout).lines().count()
    }

    pub fn has_session(&self, name: &str) -> bool {
        self.tmux(&["has-session", "-t", name]).status.success()
    }

    /// The working directory of `name`'s first pane.
    pub fn pane_directory(&self, name: &str) -> PathBuf {
        let out = self.tmux(&["display-message", "-p", "-t", name, "#{pane_current_path}"]);
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
            .canonicalize()
            .unwrap()
    }

    pub fn pane_text(&self, name: &str) -> String {
        String::from_utf8_lossy(&self.tmux(&["capture-pane", "-p", "-t", name]).stdout).into_owned()
    }
}

impl Drop for PrivateTmux {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
        std::env::remove_var("TDDY_TMUX_BINARY");
        std::env::remove_var("TMUX_TMPDIR");
    }
}

/// A daemon serving one running session whose identity maps to the OS user running the test.
pub struct TmuxTabDaemon {
    pub service: tddy_terminal_rpc::TerminalSessionServiceImpl,
    pub manager: Arc<CliSessionManager>,
    pub worktree: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

pub async fn a_daemon_with_a_running_session() -> TmuxTabDaemon {
    let (config_dir, config) = a_config_mapping_to_this_user();
    let worktree = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(CliSessionManager::new());
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
    pub async fn list_tmux_sessions(&self) -> Result<Vec<TmuxSessionInfo>, tddy_rpc::Status> {
        self.service
            .list_tmux_sessions(Request::direct(ListTmuxSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .map(|resp| resp.into_inner().sessions)
    }

    pub async fn start_new_tmux_tab(&self, name: &str) -> Result<String, tddy_rpc::Status> {
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

    pub async fn start_tmux_tab(&self, tmux_session: &str) -> Result<String, tddy_rpc::Status> {
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

    pub async fn stop_tab(&self, terminal_id: &str) {
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

    pub async fn type_into(&self, terminal_id: &str, text: &str) {
        let tab = self
            .manager
            .get_terminal(SESSION_ID, terminal_id)
            .await
            .expect("the tab is running");
        tab.stdin_tx.send(Bytes::from(text.to_string())).unwrap();
    }
}

pub async fn until_attached(tmux: &PrivateTmux, name: &str, clients: usize) {
    eventually("the tab's client count to settle", OBSERVE, || {
        let attached = tmux.attached_clients(name);
        (attached == clients)
            .then_some(())
            .ok_or_else(|| format!("{attached} clients attached, waiting for {clients}"))
    })
    .await;
}

pub fn this_os_user() -> String {
    std::env::var("USER").expect("USER names the OS user running the test")
}

pub fn a_config_mapping_to_this_user() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, "").unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        format!("- github_user: \"testuser\"\n  os_user: \"{}\"\n", this_os_user()),
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

pub async fn start_main_terminal(manager: &CliSessionManager, worktree: &Path) {
    let main = a_stub_agent_script(worktree, "stub_main.sh")
        .then_reading_stdin()
        .build();
    manager
        .start_with_options(
            SESSION_ID,
            worktree.to_path_buf(),
            "claude-opus-4-8",
            vec![main.to_str().unwrap().to_string()],
            "claude-cli",
            None,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("main terminal must start");
}

pub fn session_host(
    config: DaemonConfig,
    data_dir: PathBuf,
    manager: Arc<CliSessionManager>,
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
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    )
}
