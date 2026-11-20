//! Acceptance tests: a session is active if and only if the daemon holds at least one live agent tab
//! for it — its main agent, or an agent terminal tab — never because the pid recorded in its
//! `.session.yaml` happens to name a running process.
//!
//! Feature: docs/ft/daemon/session-liveness.md
//!
//! The recorded pid of every session here belongs to an **unrelated** process the test keeps
//! running (`sleep`), standing in for the process the OS reissued a dead agent's pid to. Harnesses
//! are stub scripts that hold their PTY open. PTY-spawning tests run serially.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    DeleteSessionRequest, ListSessionsRequest, SessionService as SessionServiceTrait, Signal,
    SignalSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::{CliSessionManager, MAIN_TERMINAL_ID};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StartTerminalSessionRequest, StopTerminalSessionRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "liveness-session";

/// A safety net for a spawned or exiting stub to be observed, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// The unrelated process a recycled pid now names
// ---------------------------------------------------------------------------

/// A process that has nothing to do with any session, running for the whole test. Its pid is what
/// a session's `.session.yaml` records — the state a recycled pid leaves behind.
struct UnrelatedProcess(Child);

fn an_unrelated_process() -> UnrelatedProcess {
    UnrelatedProcess(
        Command::new("sleep")
            .arg("300")
            .spawn()
            .expect("sleep must start"),
    )
}

impl UnrelatedProcess {
    fn pid(&self) -> u32 {
        self.0.id()
    }

    /// Read from the test's own `Child`, so the answer is about this exact process.
    fn is_running(&mut self) -> bool {
        self.0.try_wait().expect("try_wait on sleep").is_none()
    }
}

impl Drop for UnrelatedProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ---------------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------------

/// A daemon over one recorded `claude-cli` session whose `.session.yaml` names an unrelated live
/// process as its pid, with a stub Muse Code configured for agent tabs.
struct LivenessDaemon {
    host: DaemonSessionHost,
    manager: Arc<CliSessionManager>,
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    unrelated: UnrelatedProcess,
    worktree: PathBuf,
    sessions_base: PathBuf,
    stub_dir: tempfile::TempDir,
    _worktree_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
}

fn a_daemon_whose_session_records_an_unrelated_pid() -> LivenessDaemon {
    let unrelated = an_unrelated_process();
    a_daemon_recording(Some(unrelated.pid()), unrelated)
}

fn a_daemon_whose_session_records_no_pid() -> LivenessDaemon {
    a_daemon_recording(None, an_unrelated_process())
}

fn a_daemon_recording(pid: Option<u32>, unrelated: UnrelatedProcess) -> LivenessDaemon {
    let stub_dir = tempfile::tempdir().unwrap();
    let muse = a_stub_agent_script(stub_dir.path(), "muse.sh")
        .then_reading_stdin()
        .build();
    let (config_dir, config) = a_config();
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[("muse", &muse)]);
    let worktree_dir = tempfile::tempdir().unwrap();
    let worktree = worktree_dir.path().canonicalize().unwrap();
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    a_recorded_session(&sessions_base, &worktree, pid);

    let manager = Arc::new(CliSessionManager::new());
    let host = session_host(
        config.clone(),
        agents.clone(),
        sessions_base.clone(),
        manager.clone(),
    );
    LivenessDaemon {
        host,
        manager,
        config,
        agents,
        unrelated,
        worktree,
        sessions_base,
        stub_dir,
        _worktree_dir: worktree_dir,
        _config_dir: config_dir,
        _sessions_dir: sessions_dir,
    }
}

fn a_recorded_session(sessions_base: &Path, worktree: &Path, pid: Option<u32>) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let builder = a_session_metadata()
        .with_session_id(SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .with_repo_path(worktree.to_str().unwrap());
    let metadata = match pid {
        Some(pid) => builder.with_pid(pid).build(),
        None => builder.without_pid().build(),
    };
    write_session_yaml(&dir, &metadata);
}

impl LivenessDaemon {
    /// Start the session's main agent: a stub that holds its PTY open until it is stopped.
    async fn with_a_running_main_agent(self) -> Self {
        let main = a_stub_agent_script(self.stub_dir.path(), "main.sh")
            .then_reading_stdin()
            .build();
        self.start_main(&main).await;
        self
    }

    /// Start the session's main agent as a stub that exits once [`Self::the_main_agent_exits`]
    /// releases it.
    async fn with_a_main_agent_that_exits_on_release(self) -> Self {
        let release = self.release_file();
        let main = a_stub_agent_script(self.stub_dir.path(), "main.sh")
            .then_running(&format!(
                "until [ -e \"{}\" ]; do sleep 0.05; done",
                release.display()
            ))
            .build();
        self.start_main(&main).await;
        self
    }

    /// Start the session's main agent as a stub that records an interrupt it receives. Returns once
    /// the stub has installed its trap: a SIGINT sent before that reaches the default disposition
    /// and kills the stub, so the interrupt would never be recorded.
    async fn with_a_main_agent_recording_interrupts(self) -> Self {
        let record = self.interrupt_record();
        let ready = self.interrupt_ready();
        let main = a_stub_agent_script(self.stub_dir.path(), "main.sh")
            .with_prelude(&format!(
                "trap 'echo interrupted > \"{}\"' INT\ntouch \"{}\"",
                record.display(),
                ready.display()
            ))
            .then_running("while :; do sleep 0.05; done")
            .build();
        self.start_main(&main).await;
        eventually(
            "the main agent to install its interrupt trap",
            OBSERVE,
            || {
                ready
                    .exists()
                    .then_some(())
                    .ok_or_else(|| format!("{} not written", ready.display()))
            },
        )
        .await;
        self
    }

    async fn start_main(&self, binary: &Path) {
        self.manager
            .start_with_options(
                SESSION_ID,
                self.worktree.clone(),
                "claude-opus-4-8",
                vec![binary.to_str().unwrap().to_string()],
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

    fn release_file(&self) -> PathBuf {
        self.stub_dir.path().join("release-main")
    }

    fn interrupt_record(&self) -> PathBuf {
        self.stub_dir.path().join("main-interrupted")
    }

    fn interrupt_ready(&self) -> PathBuf {
        self.stub_dir.path().join("main-ready")
    }

    /// Let the main agent exit, and wait until the daemon no longer serves it.
    async fn the_main_agent_exits(&self) {
        std::fs::write(self.release_file(), "").unwrap();
        eventually_awaiting("the main agent to exit", OBSERVE, || async {
            match self.manager.get(SESSION_ID).await {
                None => Ok(()),
                Some(_) => Err("the main terminal is still served".to_string()),
            }
        })
        .await;
    }

    async fn opens_a_tab(&self, kind: &str) -> String {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: String::new(),
            }))
            .await
            .expect("StartTerminalSession must succeed")
            .into_inner()
            .terminal_id
    }

    async fn closes_the_tab(&self, terminal_id: &str) {
        self.host
            .terminal_session_service()
            .stop_terminal_session(Request::direct(StopTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("StopTerminalSession must succeed");
    }

    /// End the main agent alone. `StopTerminalSession` refuses the reserved main terminal, so this
    /// goes to the manager, as the agent's own exit would.
    async fn stops_the_main_agent(&self) {
        self.manager
            .stop_terminal(SESSION_ID, MAIN_TERMINAL_ID)
            .await;
    }

    async fn is_listed_active(&self) -> bool {
        is_listed_active_on(&self.host).await
    }

    /// The same session, as a freshly started daemon over the same sessions directory lists it.
    fn restarted(&self) -> DaemonSessionHost {
        session_host(
            self.config.clone(),
            self.agents.clone(),
            self.sessions_base.clone(),
            Arc::new(CliSessionManager::new()),
        )
    }

    async fn deletes_the_session(&self) {
        self.host
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .expect("DeleteSession must succeed");
    }

    async fn signals_the_session(&self, signal: Signal) -> Result<(), tddy_rpc::Status> {
        self.host
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                signal: signal as i32,
                control_token: String::new(),
            }))
            .await
            .map(|_| ())
    }
}

async fn is_listed_active_on(host: &DaemonSessionHost) -> bool {
    host.list_sessions(Request::direct(ListSessionsRequest {
        session_token: VALID_TOKEN.to_string(),
    }))
    .await
    .expect("ListSessions must succeed")
    .into_inner()
    .sessions
    .into_iter()
    .find(|s| s.session_id == SESSION_ID)
    .expect("the recorded session is listed")
    .is_active
}

fn a_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, "").unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        "- github_user: \"testuser\"\n  os_user: \"testuser\"\n",
    )
    .unwrap();
    let config = DaemonConfig::load(&path).expect("config must parse");
    (dir, config)
}

fn session_host(
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
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
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
        agents,
    )
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_recorded_pid_belongs_to_an_unrelated_live_process_lists_inactive() {
    // Given a session with no live agent tab, whose recorded pid now names an unrelated process
    let mut daemon = a_daemon_whose_session_records_an_unrelated_pid();
    assert!(daemon.unrelated.is_running());

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then it is inactive
    assert!(!active, "a recycled pid does not make a session active");
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_main_agent_runs_lists_active_without_a_recorded_pid() {
    // Given a session whose main agent runs, and whose `.session.yaml` records no pid
    let daemon = a_daemon_whose_session_records_no_pid()
        .with_a_running_main_agent()
        .await;

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then it is active
    assert!(active, "a running main agent makes its session active");
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_main_agent_exited_while_an_agent_tab_runs_lists_active() {
    // Given a session with a second agent tab, whose main agent then exits
    let daemon = a_daemon_whose_session_records_no_pid()
        .with_a_main_agent_that_exits_on_release()
        .await;
    daemon.opens_a_tab("muse-cli").await;
    daemon.the_main_agent_exits().await;

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then it is still active
    assert!(active, "a live agent tab keeps its session active");
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_agent_tabs_have_all_closed_lists_inactive() {
    // Given a session whose main agent and agent tab have both gone, its recorded pid still naming
    // a running process
    let daemon = a_daemon_whose_session_records_an_unrelated_pid()
        .with_a_running_main_agent()
        .await;
    let tab = daemon.opens_a_tab("muse-cli").await;
    daemon.closes_the_tab(&tab).await;
    daemon.stops_the_main_agent().await;

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then it is inactive
    assert!(!active, "a session with no live agent tab is inactive");
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_with_only_a_shell_tab_lists_inactive() {
    // Given a session whose only live terminal is a shell, its recorded pid naming a running process
    let daemon = a_daemon_whose_session_records_an_unrelated_pid();
    daemon.opens_a_tab("bash").await;

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then it is inactive
    assert!(!active, "a shell is not an agent");
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_lists_inactive_after_the_daemon_restarts() {
    // Given a session listed active, whose recorded pid names a running process
    let daemon = a_daemon_whose_session_records_an_unrelated_pid()
        .with_a_running_main_agent()
        .await;
    assert!(daemon.is_listed_active().await);

    // When the daemon restarts
    daemon.host.shut_down_children().await;
    let fresh = daemon.restarted();

    // Then the fresh daemon, which holds none of its agents, lists it inactive
    assert!(
        !is_listed_active_on(&fresh).await,
        "a restarted daemon holds no agent tab, whatever the yaml says"
    );
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_leaves_the_process_its_recorded_pid_names_running() {
    // Given a session whose recorded pid names an unrelated running process
    let mut daemon = a_daemon_whose_session_records_an_unrelated_pid();

    // When the session is deleted
    daemon.deletes_the_session().await;

    // Then that process is still running
    assert!(
        daemon.unrelated.is_running(),
        "deleting a session never signals the pid its yaml records"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_stops_its_agent_tabs() {
    // Given a session with a running main agent and an agent tab
    let daemon = a_daemon_whose_session_records_no_pid()
        .with_a_running_main_agent()
        .await;
    daemon.opens_a_tab("muse-cli").await;

    // When the session is deleted
    daemon.deletes_the_session().await;

    // Then the daemon serves none of its terminals
    eventually_awaiting("the session's terminals to stop", OBSERVE, || async {
        let left = daemon.manager.list_terminals(SESSION_ID).await.len();
        (left == 0)
            .then_some(())
            .ok_or_else(|| format!("{left} terminal(s) still served"))
    })
    .await;
}

// ---------------------------------------------------------------------------
// Signal
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn signalling_a_session_reaches_its_main_agent_not_the_recorded_pid() {
    // Given a session whose main agent runs, and whose recorded pid names an unrelated process
    let mut daemon = a_daemon_whose_session_records_an_unrelated_pid()
        .with_a_main_agent_recording_interrupts()
        .await;

    // When the session is interrupted
    daemon
        .signals_the_session(Signal::Sigint)
        .await
        .expect("SignalSession reaches a running main agent");

    // Then the main agent received it, and the unrelated process did not
    let record = daemon.interrupt_record();
    eventually("the main agent to record the interrupt", OBSERVE, || {
        record
            .exists()
            .then_some(())
            .ok_or_else(|| format!("{} not written", record.display()))
    })
    .await;
    assert!(
        daemon.unrelated.is_running(),
        "the recorded pid is never signalled"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn signalling_a_session_with_no_running_agent_is_refused_and_signals_nothing() {
    // Given a session with no live agent, whose recorded pid names an unrelated process
    let mut daemon = a_daemon_whose_session_records_an_unrelated_pid();

    // When the session is terminated
    let signalled = daemon.signals_the_session(Signal::Sigterm).await;

    // Then the signal is refused, and the unrelated process is untouched
    let refusal = signalled.expect_err("there is no agent to signal");
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(
        daemon.unrelated.is_running(),
        "the recorded pid is never signalled"
    );
}
