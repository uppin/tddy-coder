//! The fixture the jailed-tab suites (`sandbox_shell_tab_acceptance`, `sandbox_shutdown_acceptance`)
//! share: a daemon over one recorded **sandboxed** session, whose main jail is a stand-in runner
//! registered where the daemon resolves jails, and whose tab jails start through
//! [`FakeTabJailSpawn`] — so every behaviour of a jailed tab but the kernel sandbox is exercised on
//! any host. The resume suite uses the double alone, over its own fixture.
//!
//! The session's identity maps to the OS user running the test, so an "On host" tab is that user's
//! login shell.

#![allow(dead_code)]

pub mod fake_tab_jails;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use bytes::Bytes;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::tab_roster::{read_tab_roster, TabEntry, TabRoster};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_sandbox::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};
use tddy_daemon_sandbox::tab_jails::TabJailSpawn;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    DeleteSessionRequest, ListSessionsRequest, SessionService as SessionServiceTrait,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_task::TerminalCapture;
use tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode as WireLaunchNode, ListTerminalSessionsRequest, StartTerminalSessionRequest,
    StopTerminalSessionRequest, StreamReplayMode, StreamTerminalOutputRequest, TerminalSessionInfo,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_terminal_rpc::serve_stream_terminal_output_with;
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_testing_commons::wait::eventually_awaiting;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};
use tokio::sync::{broadcast, mpsc};

use crate::support::{a_config_mapping_to_this_user, OBSERVE, VALID_TOKEN};
use fake_tab_jails::{a_tab_jail_spawn, FakeTabJailSpawn};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The recorded session every tab is opened in.
pub const JAILED_SESSION_ID: &str = "sandboxed-tab-session";

/// The wrapper kind a tab opened "In sandbox" is started with.
pub const SANDBOX: &str = "sandbox";

/// A daemon over [`JAILED_SESSION_ID`], recorded as a `claude-cli` session with a worktree.
pub struct SandboxedTabDaemon {
    pub host: Arc<DaemonSessionHost>,
    /// Where the session's tab jails are started.
    pub jails: Arc<FakeTabJailSpawn>,
    pub worktree: PathBuf,
    /// The session's main jail, when it is sandboxed.
    main_jail: Option<Arc<SandboxSessionState>>,
    sessions_base: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

/// A daemon serving the session recorded as sandboxed, its main jail running and no tab open yet.
pub async fn a_sandboxed_session() -> SandboxedTabDaemon {
    let daemon = a_daemon_over_the_session(true);
    let main_jail = Arc::new(a_stand_in_main_jail(&daemon.worktree));
    daemon
        .host
        .sandbox_sessions()
        .insert(JAILED_SESSION_ID.to_string(), Arc::clone(&main_jail))
        .await;
    SandboxedTabDaemon {
        main_jail: Some(main_jail),
        ..daemon
    }
}

/// A daemon serving the session recorded as running on the host — not sandboxed — with no tab open.
pub fn an_unsandboxed_session() -> SandboxedTabDaemon {
    a_daemon_over_the_session(false)
}

fn a_daemon_over_the_session(sandboxed: bool) -> SandboxedTabDaemon {
    let (config_dir, config) = a_config_mapping_to_this_user();
    let worktree_dir = tempfile::tempdir().unwrap();
    let worktree = worktree_dir.path().canonicalize().unwrap();
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    a_recorded_session(&sessions_base, &worktree, sandboxed);
    let jails = a_tab_jail_spawn();
    let host = a_session_host(
        config,
        sessions_base.clone(),
        Arc::clone(&jails) as Arc<dyn TabJailSpawn>,
    );
    SandboxedTabDaemon {
        host,
        jails,
        worktree,
        main_jail: None,
        sessions_base,
        _dirs: vec![config_dir, worktree_dir, sessions_dir],
    }
}

fn a_recorded_session(sessions_base: &Path, worktree: &Path, sandboxed: bool) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(JAILED_SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let mut metadata = a_session_metadata()
        .with_session_id(JAILED_SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .with_repo_path(worktree.to_str().unwrap())
        .without_pid()
        .build();
    metadata.sandbox = sandboxed.then_some(true);
    write_session_yaml(&dir, &metadata);
}

fn a_session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    tab_jails: Arc<dyn TabJailSpawn>,
) -> Arc<DaemonSessionHost> {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    Arc::new(
        DaemonSessionHost::new(
            config.clone(),
            sessions_base_resolver,
            tddy_data_dir,
            user_resolver,
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
            Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
            Arc::new(CliSessionManager::new()),
            tddy_session_lifecycle::test_util::builtin_coding_agents(),
        )
        .with_tab_jail_spawn(tab_jails),
    )
}

/// The session's main jail: a `sleep` standing in for its runner.
fn a_stand_in_main_jail(worktree: &Path) -> SandboxSessionState {
    let child = Command::new("sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in main runner starts");
    let pid = child.id();
    let handle = tddy_sandbox::SandboxHandle::new(
        child,
        worktree.join("unused-profile.sb"),
        worktree.join("unused.sock"),
        worktree.join("unused.ready"),
    );
    let (stdout_tx, _) = broadcast::channel(64);
    let (stdin_tx, _) = mpsc::unbounded_channel();
    SandboxSessionState::new(SandboxSessionStateInit {
        pid,
        worktree_path: worktree.to_path_buf(),
        stdout_tx,
        capture: Arc::new(StdMutex::new(TerminalCapture::new())),
        stdin_tx,
        ready_marker: worktree.join("unused.ready"),
        handle,
        managed_workflow: None,
    })
}

/// A wrapper node as `StartTerminalSessionRequest.wrappers` carries it.
pub fn a_wrapper(kind: &str) -> WireLaunchNode {
    WireLaunchNode {
        kind: kind.to_string(),
        properties: Default::default(),
    }
}

/// The kinds of a listed terminal's launch chain, outermost first.
pub fn wire_kinds_of(terminal: &TerminalSessionInfo) -> Vec<&str> {
    terminal
        .launch
        .iter()
        .map(|node| node.kind.as_str())
        .collect()
}

/// The kinds of a recorded launch chain, outermost first.
pub fn kinds_of(entry: &TabEntry) -> Vec<&str> {
    entry.launch.iter().map(|node| node.kind.as_str()).collect()
}

impl SandboxedTabDaemon {
    pub fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(JAILED_SESSION_ID)
    }

    /// The session's roster, as the file beside its `.session.yaml` holds it.
    pub fn roster(&self) -> TabRoster {
        read_tab_roster(&self.session_dir()).expect("the session's tabs.yaml reads")
    }

    /// The roster's entry for `terminal_id`.
    pub fn roster_entry(&self, terminal_id: &str) -> TabEntry {
        self.roster()
            .tabs
            .into_iter()
            .find(|tab| tab.terminal_id == terminal_id)
            .unwrap_or_else(|| panic!("the roster records terminal {terminal_id}"))
    }

    pub fn roster_records(&self, terminal_id: &str) -> bool {
        self.roster()
            .tabs
            .iter()
            .any(|tab| tab.terminal_id == terminal_id)
    }

    /// Whether the session's main jail still runs.
    pub fn main_jail_runs(&self) -> bool {
        self.main_jail
            .as_ref()
            .expect("the session is sandboxed")
            .is_running()
    }

    /// Open a tab of `kind` "In sandbox": wrapped in a jail of its own.
    pub async fn opens_a_tab_in_sandbox(&self, kind: &str) -> Result<String, Status> {
        self.start_tab(kind, vec![a_wrapper(SANDBOX)]).await
    }

    /// Open a shell tab "In sandbox", which must start.
    pub async fn opens_a_jailed_shell(&self) -> String {
        self.opens_a_tab_in_sandbox("bash")
            .await
            .expect("a jailed shell tab starts")
    }

    /// Open a shell tab "On host": the session user's login shell, unjailed.
    pub async fn opens_a_host_shell(&self) -> String {
        self.start_tab("bash", Vec::new())
            .await
            .expect("a host shell tab starts")
    }

    async fn start_tab(&self, kind: &str, wrappers: Vec<WireLaunchNode>) -> Result<String, Status> {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: JAILED_SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: String::new(),
                wrappers,
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    pub async fn closes_the_tab(&self, terminal_id: &str) {
        self.host
            .terminal_session_service()
            .stop_terminal_session(Request::direct(StopTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: JAILED_SESSION_ID.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("StopTerminalSession must succeed");
    }

    /// Delete the session, as the web's Delete does.
    pub async fn deletes_the_session(&self) {
        let deleted = self
            .host
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: JAILED_SESSION_ID.to_string(),
            }))
            .await
            .expect("DeleteSession must succeed")
            .into_inner();
        assert!(deleted.ok, "the session was deleted");
    }

    /// Stop the daemon's children, as its SIGTERM handler does.
    pub async fn shuts_down(&self) {
        self.host.shut_down_children().await;
    }

    /// The session's terminals, as `ListTerminalSessions` reports them.
    pub async fn listed_terminals(&self) -> Vec<TerminalSessionInfo> {
        self.host
            .terminal_session_service()
            .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: JAILED_SESSION_ID.to_string(),
            }))
            .await
            .expect("ListTerminalSessions must succeed")
            .into_inner()
            .terminals
    }

    /// The listed terminal `terminal_id`.
    pub async fn listed_terminal(&self, terminal_id: &str) -> TerminalSessionInfo {
        let listed = self.listed_terminals().await;
        listed
            .iter()
            .find(|terminal| terminal.terminal_id == terminal_id)
            .cloned()
            .unwrap_or_else(|| panic!("terminal {terminal_id} is not listed: {listed:?}"))
    }

    pub async fn listed_terminal_ids(&self) -> BTreeSet<String> {
        self.listed_terminals()
            .await
            .into_iter()
            .map(|terminal| terminal.terminal_id)
            .collect()
    }

    /// Wait until `ListTerminalSessions` no longer reports `terminal_id`.
    pub async fn until_unlisted(&self, terminal_id: &str) {
        eventually_awaiting(
            &format!("terminal {terminal_id} to end"),
            OBSERVE,
            || async {
                let listed = self.listed_terminal_ids().await;
                (!listed.contains(terminal_id))
                    .then_some(())
                    .ok_or_else(|| format!("still listed: {listed:?}"))
            },
        )
        .await;
    }

    /// Whether `ListSessions` reports the session as sandboxed.
    pub async fn is_listed_sandboxed(&self) -> bool {
        self.host
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions must succeed")
            .into_inner()
            .sessions
            .into_iter()
            .find(|session| session.session_id == JAILED_SESSION_ID)
            .expect("the recorded session is listed")
            .sandbox
    }

    /// Type `text` into terminal `terminal_id`, through the store every terminal RPC resolves it in.
    pub async fn types_into(&self, terminal_id: &str, text: &str) {
        self.host
            .terminal_sessions()
            .get_terminal(JAILED_SESSION_ID, terminal_id)
            .await
            .unwrap_or_else(|| panic!("terminal {terminal_id} is not served"))
            .send_input(Bytes::from(text.to_string()), 0);
    }

    /// Open `StreamTerminalOutput` on terminal `terminal_id` from its oldest retained byte, and read
    /// it until what it streamed contains `text`; what it streamed by then.
    pub async fn until_streamed_output_contains(&self, terminal_id: &str, text: &str) -> String {
        let request = StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: JAILED_SESSION_ID.to_string(),
            terminal_id: terminal_id.to_string(),
            initial_cols: 120,
            initial_rows: 40,
            mode: StreamReplayMode::FromOffset as i32,
            from_offset: 0,
            control_token: String::new(),
        };
        let mut frames = serve_stream_terminal_output_with(
            &self.host.terminal_sessions(),
            request,
            DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await
        .unwrap_or_else(|status| panic!("terminal {terminal_id} is not streamed: {status:?}"));
        let mut streamed = String::new();
        let read = tokio::time::timeout(OBSERVE, async {
            while !streamed.contains(text) {
                match frames.recv().await {
                    Some(Ok(frame)) => streamed.push_str(&String::from_utf8_lossy(&frame.data)),
                    Some(Err(status)) => panic!("the stream of {terminal_id} errored: {status:?}"),
                    None => {
                        panic!("the stream of {terminal_id} ended having streamed {streamed:?}")
                    }
                }
            }
        })
        .await;
        assert!(
            read.is_ok(),
            "terminal {terminal_id} never streamed {text:?}; it streamed {streamed:?}"
        );
        streamed
    }
}
