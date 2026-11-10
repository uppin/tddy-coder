//! Acceptance tests: a stopped session keeps its tabs — `StartTerminalSession` on a session with no
//! running main terminal starts the tab in the worktree the session's record names.
//!
//! Feature: docs/ft/web/session-terminal-tabs.md
//! Feature: docs/ft/daemon/terminal-sessions.md § Start / Stop
//!
//! A stopped session is a session directory under the daemon's data dir whose `.session.yaml`
//! names a worktree, and no `main` terminal in the manager. Harnesses are stub scripts that record
//! their working directory and then hold their PTY open. PTY-spawning tests run serially.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StartTerminalSessionRequest, TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "stopped-tab-session";

/// A safety net for a spawned stub to record itself, not a prediction of how long it takes.
const SPAWN_OBSERVE: Duration = Duration::from_secs(10);

/// A daemon with a stub Muse Code harness configured, over a data dir the test writes sessions into.
struct DormantTabDaemon {
    service: tddy_terminal_rpc::TerminalSessionServiceImpl,
    manager: Arc<CliSessionManager>,
    data_dir: PathBuf,
    muse_cwd_log: PathBuf,
    _data_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

/// A checkout on disk, canonicalized so it compares with what a spawned process reports.
struct Worktree {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_worktree() -> Worktree {
    let dir = tempfile::tempdir().unwrap();
    Worktree {
        path: dir.path().canonicalize().unwrap(),
        _dir: dir,
    }
}

fn a_daemon() -> DormantTabDaemon {
    let stub_dir = tempfile::tempdir().unwrap();
    let muse_cwd_log = stub_dir.path().join("muse.cwd.log");
    let muse = a_stub_agent_script(stub_dir.path(), "muse.sh")
        .with_prelude(&format!("pwd -P > \"{}\"", muse_cwd_log.display()))
        .then_reading_stdin()
        .build();
    let (config_dir, config) = a_config_naming_muse(&muse);
    let data_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(CliSessionManager::new());
    let service = session_host(config, data_dir.path().to_path_buf(), Arc::clone(&manager))
        .terminal_session_service();
    DormantTabDaemon {
        service,
        manager,
        data_dir: data_dir.path().to_path_buf(),
        muse_cwd_log,
        _data_dir: data_dir,
        _stub_dir: stub_dir,
        _config_dir: config_dir,
    }
}

impl DormantTabDaemon {
    /// Record `SESSION_ID` as a stopped session whose metadata names `repo_path` (or no worktree).
    fn with_a_stopped_session_recording(self, repo_path: Option<&Path>) -> Self {
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(&self.data_dir, SESSION_ID);
        std::fs::create_dir_all(&session_dir).unwrap();
        let repo_line = repo_path
            .map(|path| format!("repo_path: \"{}\"\n", path.display()))
            .unwrap_or_default();
        std::fs::write(
            session_dir.join(".session.yaml"),
            format!(
                "session_id: {SESSION_ID}\nproject_id: p1\ncreated_at: \"2026-11-02T00:00:00Z\"\n\
                 updated_at: \"2026-11-02T00:00:00Z\"\nstatus: exited\nsession_type: claude-cli\n\
                 {repo_line}"
            ),
        )
        .unwrap();
        self
    }

    /// Run the session's main agent — a stub holding its PTY open — in `worktree`.
    async fn with_its_main_agent_running_in(self, worktree: &Path) -> Self {
        let main = a_stub_agent_script(worktree, "stub_main.sh")
            .then_reading_stdin()
            .build();
        self.manager
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
        self
    }

    async fn start_terminal(
        &self,
        session_id: &str,
        kind: &str,
    ) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: String::new(),
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    /// The worktree the manager registered a started terminal under.
    async fn worktree_of(&self, terminal_id: &str) -> PathBuf {
        self.manager
            .list_terminals(SESSION_ID)
            .await
            .into_iter()
            .find(|handle| handle.terminal_id == terminal_id)
            .unwrap_or_else(|| panic!("terminal {terminal_id} is not running"))
            .worktree_path
            .clone()
    }

    /// The directory the stub Muse Code last ran in, waiting for it to record it.
    async fn muse_working_directory(&self) -> PathBuf {
        eventually("the stub harness to record its cwd", SPAWN_OBSERVE, || {
            std::fs::read_to_string(&self.muse_cwd_log)
                .map(|cwd| PathBuf::from(cwd.trim()))
                .map_err(|e| format!("{} is not readable yet: {e}", self.muse_cwd_log.display()))
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// A stopped session
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_bash_tab_on_a_stopped_session_starts_in_its_recorded_worktree() {
    // Given
    let worktree = a_worktree();
    let daemon = a_daemon().with_a_stopped_session_recording(Some(&worktree.path));

    // When
    let terminal_id = daemon
        .start_terminal(SESSION_ID, "")
        .await
        .expect("a bash tab starts on a stopped session");

    // Then
    assert_eq!(daemon.worktree_of(&terminal_id).await, worktree.path);
}

#[tokio::test]
#[serial_test::serial]
async fn a_harness_tab_on_a_stopped_session_starts_in_its_recorded_worktree() {
    // Given
    let worktree = a_worktree();
    let daemon = a_daemon().with_a_stopped_session_recording(Some(&worktree.path));

    // When
    daemon
        .start_terminal(SESSION_ID, "muse-cli")
        .await
        .expect("a Muse tab starts on a stopped session");

    // Then
    assert_eq!(daemon.muse_working_directory().await, worktree.path);
}

#[tokio::test]
#[serial_test::serial]
async fn a_stopped_session_without_a_recorded_worktree_is_refused_naming_it() {
    // Given
    let daemon = a_daemon().with_a_stopped_session_recording(None);

    // When
    let refusal = daemon.start_terminal(SESSION_ID, "").await.unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains(SESSION_ID),
        "{}",
        refusal.message()
    );
    assert!(daemon.manager.list_terminals(SESSION_ID).await.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn an_unknown_session_is_refused_naming_it() {
    // Given
    let daemon = a_daemon();

    // When
    let refusal = daemon
        .start_terminal("never-created", "")
        .await
        .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("never-created"),
        "{}",
        refusal.message()
    );
}

// ---------------------------------------------------------------------------
// A live session is unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_live_session_still_starts_tabs_in_its_main_terminals_worktree() {
    // Given a record naming one checkout while the live agent works in another
    let recorded = a_worktree();
    let live = a_worktree();
    let daemon = a_daemon()
        .with_a_stopped_session_recording(Some(&recorded.path))
        .with_its_main_agent_running_in(&live.path)
        .await;

    // When
    let terminal_id = daemon.start_terminal(SESSION_ID, "").await.unwrap();

    // Then the live agent's worktree wins
    assert_eq!(daemon.worktree_of(&terminal_id).await, live.path);
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A daemon config, and the coding agents the host installs: `muse` is the stub.
type AConfigAndItsAgents = (
    DaemonConfig,
    Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
);

fn a_config_naming_muse(muse: &Path) -> (tempfile::TempDir, AConfigAndItsAgents) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "";
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        std::path::Path::new(&(path))
            .parent()
            .expect("a config directory")
            .join("users.yaml"),
        "- github_user: \"testuser\"\n  os_user: \"testuser\"\n",
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[("muse", muse)]);
    (dir, (config, agents))
}

fn session_host(
    (config, agents): AConfigAndItsAgents,
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
        agents,
    )
}
