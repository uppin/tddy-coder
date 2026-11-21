//! The fixture `target_shell_tab_acceptance` builds on: a daemon over one recorded **target**
//! session — its `.session.yaml` names an ssh target and a worktree that exists only there — whose
//! target shells resolve through [`FakeTabTargetShells`], so every behaviour of a target tab but the
//! transport is exercised on any host. The resume suite uses the double alone, over its own fixture.
//!
//! The session's identity maps to the OS user running the test, so an "On this host" tab is that
//! user's login shell.

#![allow(dead_code)]

pub mod fake_target_shells;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_agent_launch::tab_target_shell::TabTargetShell;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::tab_roster::{read_tab_roster, TabEntry, TabRoster};
use tddy_core::SessionTarget;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Request, Status};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode as WireLaunchNode, ListTerminalSessionsRequest, StartTerminalSessionRequest,
    TerminalSessionInfo, TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::eventually;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

use crate::support::{a_config_mapping_to_this_user, OBSERVE, VALID_TOKEN};
use fake_target_shells::FakeTabTargetShells;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Who the test's session token resolves to: the session's owner.
pub const OWNER: &str = "testuser";

/// The recorded target session every tab is opened in.
pub const TARGET_SESSION_ID: &str = "target-tab-session";

/// The wrapper kind a tab opened "On target" is started with.
pub const TARGET: &str = "target";

/// The session's worktree as its record names it: a path on the target, not on this host.
pub const RECORDED_TARGET_WORKTREE: &str = "/home/dev/repo/.worktrees/target-tab-session";

/// A daemon over [`TARGET_SESSION_ID`], recorded as a `claude-cli` session on an ssh target.
pub struct TargetTabDaemon {
    pub host: Arc<DaemonSessionHost>,
    /// Where the session's target shells resolve.
    pub target_shells: Arc<FakeTabTargetShells>,
    /// The daemon's terminals, as the host spawned them.
    terminals: Arc<CliSessionManager>,
    sessions_base: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

/// A daemon serving the target session, its target shells resolving through `target_shells`, and no
/// tab open yet.
pub fn a_target_session_whose_shells_resolve_through(
    target_shells: Arc<FakeTabTargetShells>,
) -> TargetTabDaemon {
    let (config_dir, config) = a_config_mapping_to_this_user();
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    a_recorded_target_session(&sessions_base);
    let terminals = Arc::new(CliSessionManager::new());
    let host = a_session_host(
        config,
        sessions_base.clone(),
        Arc::clone(&terminals),
        Arc::clone(&target_shells) as Arc<dyn TabTargetShell>,
    );
    TargetTabDaemon {
        host,
        target_shells,
        terminals,
        sessions_base,
        _dirs: vec![config_dir, sessions_dir],
    }
}

fn a_recorded_target_session(sessions_base: &Path) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(TARGET_SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let mut metadata = a_session_metadata()
        .with_session_id(TARGET_SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .with_repo_path(RECORDED_TARGET_WORKTREE)
        .without_pid()
        .build();
    metadata.target = Some(SessionTarget::ssh("buildbox"));
    write_session_yaml(&dir, &metadata);
}

fn a_session_host(
    config: DaemonConfig,
    sessions_base: PathBuf,
    terminals: Arc<CliSessionManager>,
    target_shells: Arc<dyn TabTargetShell>,
) -> Arc<DaemonSessionHost> {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| OWNER.to_string()));
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
            terminals,
            tddy_session_lifecycle::test_util::builtin_coding_agents(),
        )
        .with_tab_target_shell(target_shells),
    )
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

impl TargetTabDaemon {
    pub fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(TARGET_SESSION_ID)
    }

    /// Who the session's target shells were asked for: its directory, as its owner — the caller's
    /// subject — once per ask.
    pub fn the_sessions_owner_asked(&self, times: usize) -> Vec<(PathBuf, String)> {
        vec![(self.session_dir(), OWNER.to_string()); times]
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

    /// Open a tab of `kind` "On target": wrapped in the session's target.
    pub async fn opens_a_tab_on_target(&self, kind: &str) -> Result<String, Status> {
        self.start_tab(kind, vec![a_wrapper(TARGET)]).await
    }

    /// Open a shell tab "On this host": the session user's login shell here, unwrapped.
    pub async fn opens_a_shell_on_this_host(&self) -> Result<String, Status> {
        self.start_tab("bash", Vec::new()).await
    }

    async fn start_tab(&self, kind: &str, wrappers: Vec<WireLaunchNode>) -> Result<String, Status> {
        self.host
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: TARGET_SESSION_ID.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: String::new(),
                wrappers,
            }))
            .await
            .map(|resp| resp.into_inner().terminal_id)
    }

    /// The directory on this host the terminal `terminal_id` was started in.
    pub async fn directory_of(&self, terminal_id: &str) -> PathBuf {
        self.terminals
            .get_terminal(TARGET_SESSION_ID, terminal_id)
            .await
            .unwrap_or_else(|| panic!("terminal {terminal_id} runs on this host"))
            .worktree_path
            .clone()
    }

    /// The session's terminals, as `ListTerminalSessions` reports them.
    pub async fn listed_terminals(&self) -> Vec<TerminalSessionInfo> {
        self.host
            .terminal_session_service()
            .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: TARGET_SESSION_ID.to_string(),
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
}

/// Where `machine`'s login shell started, once it has.
pub async fn where_the_login_shell_started(
    machine: &fake_target_shells::AStandInTarget,
) -> PathBuf {
    eventually("the target's login shell to start", OBSERVE, || {
        machine
            .login_shell_started_in()
            .ok_or_else(|| "the target's login shell has not started yet".to_string())
    })
    .await
}
