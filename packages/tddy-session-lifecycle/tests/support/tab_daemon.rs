//! The fixture the tab-roster suites (`tab_roster_acceptance`, `tmux_wrapped_tab_acceptance`) share:
//! a daemon over one recorded session on a private tmux server, whose Muse Code and Claude are stub
//! scripts that log each launch, and a restart of it over the same sessions directory.
//!
//! The session's identity maps to the OS user running the test, so its tmux server is this test's
//! private one. Every suite using it is serial: the tmux socket directory is process-wide.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::tab_roster::{read_tab_roster, TabEntry, TabRoster};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{ListSessionsRequest, SessionService as SessionServiceTrait};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode as WireLaunchNode, ListTerminalSessionsRequest, StartTerminalSessionRequest,
    StopTerminalSessionRequest, TerminalSessionInfo,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, appended_invocation_count};
use tddy_testing_commons::wait::eventually_awaiting;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

use super::{a_config_mapping_to_this_user, PrivateTmux, OBSERVE, VALID_TOKEN};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The recorded session every tab is opened in.
pub const TAB_SESSION_ID: &str = "tab-roster-session";

/// How often a detached tmux-wrapped tab is checked for its tmux session being gone: short, so a
/// test sees the end without waiting the production interval.
pub const DETACH_WATCH: Duration = Duration::from_millis(200);

/// A daemon over [`TAB_SESSION_ID`], recorded as a `claude-cli` session with a worktree and no pid.
pub struct TabDaemon {
    pub host: Arc<DaemonSessionHost>,
    pub manager: Arc<CliSessionManager>,
    pub worktree: PathBuf,
    pub tmux: PrivateTmux,
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    sessions_base: PathBuf,
    stub_dir: tempfile::TempDir,
    muse_launches: PathBuf,
    claude_launches: PathBuf,
    claude: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

/// A daemon serving the recorded session, with no terminal of it running yet.
pub fn a_tab_daemon() -> TabDaemon {
    let tmux = PrivateTmux::started();
    let stub_dir = tempfile::tempdir().unwrap();
    let muse_launches = stub_dir.path().join("muse-launches.log");
    let claude_launches = stub_dir.path().join("claude-launches.log");
    let release = stub_dir.path().join("release-tabs");
    let muse = a_stub_agent_script(stub_dir.path(), "muse.sh")
        .appending_argv_to(&muse_launches)
        .then_running(&format!(
            "until [ -e \"{}\" ]; do sleep 0.05; done",
            release.display()
        ))
        .build();
    let claude = a_stub_agent_script(stub_dir.path(), "claude.sh")
        .appending_argv_to(&claude_launches)
        .then_reading_stdin()
        .build();
    let (config_dir, config) = a_config_mapping_to_this_user();
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[
        ("muse", &muse),
        ("claude", &claude),
    ]);
    let worktree_dir = tempfile::tempdir().unwrap();
    let worktree = worktree_dir.path().canonicalize().unwrap();
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    a_recorded_session(&sessions_base, &worktree, None);
    let manager = Arc::new(a_manager());
    let host = a_session_host(
        config.clone(),
        agents.clone(),
        sessions_base.clone(),
        Arc::clone(&manager),
    );
    TabDaemon {
        host,
        manager,
        worktree,
        tmux,
        config,
        agents,
        sessions_base,
        stub_dir,
        muse_launches,
        claude_launches,
        claude,
        _dirs: vec![config_dir, worktree_dir, sessions_dir],
    }
}

fn a_manager() -> CliSessionManager {
    CliSessionManager::new().with_detach_watch_interval(DETACH_WATCH)
}

fn a_recorded_session(sessions_base: &Path, worktree: &Path, tmux_session: Option<&str>) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(TAB_SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let mut metadata = a_session_metadata()
        .with_session_id(TAB_SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .with_repo_path(worktree.to_str().unwrap())
        .without_pid()
        .build();
    metadata.tmux_session = tmux_session.map(str::to_string);
    write_session_yaml(&dir, &metadata);
}

fn a_session_host(
    config: DaemonConfig,
    agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    sessions_base: PathBuf,
    manager: Arc<CliSessionManager>,
) -> Arc<DaemonSessionHost> {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    Arc::new(DaemonSessionHost::new(
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
    ))
}

/// A wrapper node as `StartTerminalSessionRequest.wrappers` carries it.
pub fn a_wrapper(kind: &str) -> WireLaunchNode {
    WireLaunchNode {
        kind: kind.to_string(),
        properties: Default::default(),
    }
}

impl TabDaemon {
    pub fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(TAB_SESSION_ID)
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

    /// Whether the roster has an entry for `terminal_id`.
    pub fn roster_records(&self, terminal_id: &str) -> bool {
        self.roster()
            .tabs
            .iter()
            .any(|tab| tab.terminal_id == terminal_id)
    }

    /// Record the session as one whose main ran in the tmux session `name`, before tab rosters
    /// existed: `.session.yaml` names it, and no `tabs.yaml` is written.
    pub fn records_a_pre_roster_tmux_main(&self, name: &str) {
        a_recorded_session(&self.sessions_base, &self.worktree, Some(name));
    }

    /// Start the stub Claude in a detached tmux session `name` in the session's worktree, as a
    /// tmux-run main is started, and wait for it to have launched.
    pub async fn runs_claude_in_tmux_session(&self, name: &str) {
        let started = self.tmux.tmux(&[
            "new-session",
            "-d",
            "-s",
            name,
            "-c",
            self.worktree.to_str().unwrap(),
            "--",
            self.claude.to_str().unwrap(),
        ]);
        assert!(started.status.success(), "tmux new-session must succeed");
        let launches = self.claude_launches.clone();
        eventually_awaiting("the stub Claude to launch in tmux", OBSERVE, || {
            let launches = launches.clone();
            async move {
                (appended_invocation_count(&launches) == 1)
                    .then_some(())
                    .ok_or_else(|| "the stub Claude has not launched".to_string())
            }
        })
        .await;
    }

    /// How many times the stub Muse Code has been launched.
    pub fn muse_launch_count(&self) -> usize {
        appended_invocation_count(&self.muse_launches)
    }

    /// How many times the stub Claude has been launched.
    pub fn claude_launch_count(&self) -> usize {
        appended_invocation_count(&self.claude_launches)
    }

    /// Let every stub Muse Code tab exit on its own.
    pub fn muse_tabs_exit(&self) {
        std::fs::write(self.stub_dir.path().join("release-tabs"), "").unwrap();
    }

    /// Start the session's main agent as a stub that exits once [`Self::the_main_agent_exits`]
    /// releases it.
    pub async fn with_a_main_agent_that_exits_on_release(self) -> Self {
        let release = self.stub_dir.path().join("release-main");
        let main = a_stub_agent_script(self.stub_dir.path(), "main.sh")
            .then_running(&format!(
                "until [ -e \"{}\" ]; do sleep 0.05; done",
                release.display()
            ))
            .build();
        self.manager
            .start_with_options(
                TAB_SESSION_ID,
                self.worktree.clone(),
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

    /// Let the main agent exit, and wait until the daemon no longer serves it.
    pub async fn the_main_agent_exits(&self) {
        std::fs::write(self.stub_dir.path().join("release-main"), "").unwrap();
        eventually_awaiting("the main agent to exit", OBSERVE, || async {
            match self.manager.get(TAB_SESSION_ID).await {
                None => Ok(()),
                Some(_) => Err("the main terminal is still served".to_string()),
            }
        })
        .await;
    }

    /// Open a tab of `kind` running directly.
    pub async fn opens_a_tab(&self, kind: &str) -> String {
        self.start_tab(kind, "", Vec::new())
            .await
            .expect("StartTerminalSession must succeed")
    }

    /// Open a tab attached to the user's own tmux session `name`.
    pub async fn opens_a_tmux_attach_tab(&self, name: &str) -> String {
        self.start_tab("tmux", name, Vec::new())
            .await
            .expect("StartTerminalSession must succeed")
    }

    /// Open a tab of `kind` wrapped in `wrappers` (outermost first).
    pub async fn opens_a_tab_wrapped_in(
        &self,
        kind: &str,
        wrappers: &[&str],
    ) -> Result<String, tddy_rpc::Status> {
        self.start_tab(kind, "", wrappers.iter().map(|w| a_wrapper(w)).collect())
            .await
    }

    async fn start_tab(
        &self,
        kind: &str,
        tmux_session: &str,
        wrappers: Vec<WireLaunchNode>,
    ) -> Result<String, tddy_rpc::Status> {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: TAB_SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: tmux_session.to_string(),
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
                session_id: TAB_SESSION_ID.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("StopTerminalSession must succeed");
    }

    /// The session's terminals, as `ListTerminalSessions` reports them.
    pub async fn listed_terminals(&self) -> Vec<TerminalSessionInfo> {
        self.host
            .terminal_session_service()
            .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: TAB_SESSION_ID.to_string(),
            }))
            .await
            .expect("ListTerminalSessions must succeed")
            .into_inner()
            .terminals
    }

    /// Wait until the daemon no longer serves `terminal_id`.
    pub async fn until_the_tab_is_gone(&self, terminal_id: &str) {
        eventually_awaiting("the tab to leave the registry", OBSERVE, || async {
            match self.manager.get_terminal(TAB_SESSION_ID, terminal_id).await {
                None => Ok(()),
                Some(_) => Err(format!("terminal {terminal_id} is still served")),
            }
        })
        .await;
    }

    /// A daemon restarted over the same sessions directory and tmux server: no terminals
    /// registered, nothing of the old daemon listening on its agent-tool sockets.
    pub fn restarted(&self) -> Arc<DaemonSessionHost> {
        self.host.agent_tool_sockets().stop_serving();
        a_session_host(
            self.config.clone(),
            self.agents.clone(),
            self.sessions_base.clone(),
            Arc::new(a_manager()),
        )
    }

    pub async fn is_listed_active(&self) -> bool {
        is_listed_active_on(&self.host).await
    }
}

/// Whether `host` lists [`TAB_SESSION_ID`] as active.
pub async fn is_listed_active_on(host: &DaemonSessionHost) -> bool {
    host.list_sessions(Request::direct(ListSessionsRequest {
        session_token: VALID_TOKEN.to_string(),
    }))
    .await
    .expect("ListSessions must succeed")
    .into_inner()
    .sessions
    .into_iter()
    .find(|s| s.session_id == TAB_SESSION_ID)
    .expect("the recorded session is listed")
    .is_active
}

/// The kinds of a launch chain, outermost first.
pub fn kinds_of(launch: &[tddy_core::tab_roster::LaunchNode]) -> Vec<&str> {
    launch.iter().map(|node| node.kind.as_str()).collect()
}
