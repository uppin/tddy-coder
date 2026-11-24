//! What this daemon hands `tddy-terminal-rpc` so that crate can serve
//! `terminal_session.TerminalSessionService` itself.
//!
//! The nine terminal methods are `tddy-terminal-rpc`'s; what stays here is the four answers only a
//! daemon has — which terminal a `(session_id, terminal_id)` names, who holds the control lease,
//! how a login shell becomes a task, and which identity a session token belongs to. Each is a port
//! implemented once here rather than a branch repeated per handler, which is what the four terminal
//! RPCs in [`super::rpc_service`] used to carry.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{
    ControlChange, ControlClaim, MainReattach, TerminalControl, TerminalDescriptor, TerminalLaunch,
    TerminalRoster, TerminalSessionPorts, TmuxHost,
};
use tokio::sync::broadcast;

use super::DaemonSessionHost;
use crate::cli_session_manager::{CliSessionManager, ReattachOutcome};
use crate::terminal_session_adapter::DaemonTerminalSessionStore;

mod jailed_tabs;
mod recorded_tabs;
mod recorded_worktree;
pub(crate) mod session_tabs;
mod target_agent_tabs;
mod target_tabs;
mod target_tmux_tabs;
mod tmux_host;

impl DaemonSessionHost {
    /// The terminal manager this host starts and holds sessions' terminals in, for
    /// [`crate::test_util`] to hold a session live.
    pub(crate) fn terminal_manager(&self) -> &Arc<CliSessionManager> {
        &self.cli_manager
    }

    /// Every terminal this daemon serves, behind one
    /// [`TerminalSessionStore`](tddy_terminal_rpc::session::TerminalSessionStore).
    ///
    /// Composite over both owners so a *sandboxed* session's PTY is an ordinary terminal to the
    /// bridge. Before `#unbundle` node 6 each terminal RPC branched on the sandbox registry itself,
    /// and `StreamTerminalOutput`'s branch carried its own transcription of the bridge's
    /// replay/offset loop — two implementations of the same offsets, in a surface the repo has
    /// already been bitten twice by disagreeing about.
    ///
    /// Public so a daemon-side reader that is not a terminal RPC — the project-home agent's
    /// `ReadSessionTerminal` — resolves a session's `main` terminal through the same store the
    /// terminal RPCs do, sandbox first, rather than assembling a lookalike (`#home-inbox` 2/8).
    #[must_use]
    pub fn terminal_sessions(&self) -> DaemonTerminalSessionStore {
        DaemonTerminalSessionStore::new(
            Arc::clone(&self.cli_manager),
            Arc::clone(&self.sandbox_manager),
        )
        .with_tab_jails(Arc::clone(&self.tab_jails))
    }

    /// Start every jailed shell tab's jail through `spawn` instead of the launch topic's own jail
    /// launch (builder pattern) — lets a test prove what a jailed tab does without a kernel sandbox
    /// on the machine running it (`#session-liveness` 5/8).
    pub fn with_tab_jail_spawn(
        mut self,
        spawn: Arc<dyn tddy_daemon_sandbox::tab_jails::TabJailSpawn>,
    ) -> Self {
        self.tab_jail_spawn = Some(spawn);
        self
    }

    /// Resolve every target session's target for its tabs through `shells` instead of the launch
    /// topic's own (builder pattern) — lets a test prove what a target tab does without a reachable
    /// target or an unlocked vault (`#session-liveness` 6/8).
    pub fn with_tab_target_shell(
        mut self,
        shells: Arc<dyn tddy_agent_launch::tab_target_shell::TabTargetShell>,
    ) -> Self {
        self.tab_target_shell = Some(shells);
        self
    }

    /// The single-screen control lease of every terminal this daemon serves — the one
    /// `terminal_session.TerminalSessionService` claims and verifies against, so a holder named
    /// here is the holder a browser sees.
    ///
    /// Public so a daemon-side writer that is not a terminal RPC — the project-home agent's
    /// `ClaimSessionTerminal` and `SendSessionKeys` — takes and checks the same lease the browsers
    /// fight over, rather than keeping a lookalike (`#home-inbox` 5/8).
    #[must_use]
    pub fn terminal_control(&self) -> Arc<dyn TerminalControl> {
        Arc::new(CliManagerTerminalControl::new(Arc::clone(
            &self.cli_manager,
        )))
    }

    /// Every agent end on this host from now on, so a holder of per-session state outside the
    /// terminal manager (a project-home's terminal claims) forgets a session that ended
    /// (`#orchestra` 7/9).
    #[must_use]
    pub fn agent_ends(
        &self,
    ) -> tokio::sync::broadcast::Receiver<tddy_cli_sessions::cli_session_manager::AgentEnd> {
        self.cli_manager.agent_ends()
    }

    /// The jailed sessions this daemon runs, as the terminal surface resolves them.
    ///
    /// Public for the same reason [`Self::terminal_session_service`] is: a jail's PTY is reached
    /// through this registry and nowhere else, so asking what the coordinate does with one means
    /// registering it *here*. A suite that assembled its own registry would be asserting about a
    /// lookalike whose terminals this daemon's handlers never resolve.
    #[must_use]
    pub fn sandbox_sessions(
        &self,
    ) -> Arc<tddy_daemon_sandbox::sandbox_session::SandboxSessionManager> {
        Arc::clone(&self.sandbox_manager)
    }

    /// The `terminal_session.TerminalSessionService` entry this daemon registers.
    ///
    /// Built from the *same* managers every other part of this daemon reaches a PTY through, so the
    /// coordinate and a session's own lifecycle address one set of terminals and one control
    /// lease — a second `CliSessionManager` here would mean a terminal started through the
    /// coordinate was invisible to the session that owns it.
    pub fn terminal_session_entry(&self) -> tddy_rpc::ServiceEntry {
        tddy_terminal_rpc::build_terminal_session_entry(self.terminal_session_ports())
    }

    /// The served implementation itself, without the transport entry around it.
    ///
    /// Public because it is the only way to ask *this daemon's* terminal coordinate a typed
    /// question: an acceptance test that re-assembled the ports would be asserting about a
    /// lookalike, and the four answers below are exactly what a terminal RPC's behaviour turns on.
    #[must_use]
    pub fn terminal_session_service(&self) -> tddy_terminal_rpc::TerminalSessionServiceImpl {
        tddy_terminal_rpc::TerminalSessionServiceImpl::new(self.terminal_session_ports())
    }

    /// The four answers only a daemon has: which identity a token belongs to, which OS user that
    /// maps to, where the terminals and the control lease live, and what this host frames at.
    fn terminal_session_ports(&self) -> TerminalSessionPorts {
        let config = self.config.clone();
        let tmux = self.session_tmux();
        TerminalSessionPorts {
            github_users: self.user_resolver.clone(),
            os_users: Arc::new(move |github_user: &str| config.os_user_for_github(github_user)),
            terminals: Arc::new(self.terminal_sessions()),
            control: self.terminal_control(),
            roster: Arc::new(self.terminal_roster(Arc::clone(&tmux))),
            recorded_history: Arc::new(
                crate::session_dir_recorded_history::SessionDirRecordedHistory::new(
                    self.tddy_data_dir.clone(),
                ),
            ),
            main_reattach: Arc::new(TmuxMainReattach::new(
                Arc::clone(&self.cli_manager),
                self.coding_agents.clone(),
                self.tddy_data_dir.clone(),
                Arc::clone(&self.agent_tool_sockets),
            )),
            tmux,
            initial_frame_bytes: tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        }
    }
}

impl DaemonSessionHost {
    /// The host's tmux, as the terminal surface asks it.
    fn session_tmux(&self) -> Arc<dyn TmuxHost> {
        Arc::new(tmux_host::SessionTmux {
            tddy_data_dir: self.tddy_data_dir.clone(),
            target_shell: self.launch_sessions().tab_target_shell(),
        })
    }

    /// The roster every tab of this host's sessions is started, closed and ended through.
    fn terminal_roster(&self, tmux: Arc<dyn TmuxHost>) -> CliManagerTerminalRoster {
        CliManagerTerminalRoster::new(
            Arc::clone(&self.cli_manager),
            self.launch_coding_agents(),
            self.tddy_data_dir.clone(),
            tmux,
            Arc::clone(&self.session_closes),
        )
        .with_tab_jails(
            Arc::clone(&self.tab_jails),
            self.launch_sessions().tab_jail_spawn(),
        )
        .with_tab_target_shell(self.launch_sessions().tab_target_shell())
    }
}

/// Re-attaches the main terminal of a tmux-run session whose agent outlived its client or its
/// daemon, on the first access that finds no main registered.
///
/// Single-flight per session: concurrent first accesses wait on one lock, and the second finds the
/// main the first attached.
pub(super) struct TmuxMainReattach {
    manager: Arc<CliSessionManager>,
    /// The coding agents this daemon installed: what a record is re-attached as.
    coding_agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    tddy_data_dir: PathBuf,
    /// Served again for the session's OS user before its main is re-attached: the agent that
    /// outlived its daemon reports its hooks and makes its tool calls over that user's socket, and
    /// a fresh daemon serves a user's socket only once something asks for it.
    agent_tool_sockets: Arc<tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets>,
    in_flight: tokio::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl TmuxMainReattach {
    pub(super) fn new(
        manager: Arc<CliSessionManager>,
        coding_agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
        tddy_data_dir: PathBuf,
        agent_tool_sockets: Arc<tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets>,
    ) -> Self {
        TmuxMainReattach {
            manager,
            coding_agents,
            tddy_data_dir,
            agent_tool_sockets,
            in_flight: tokio::sync::Mutex::default(),
        }
    }

    /// Serve `os_user`'s agent-tool socket, which the outliving agent's hooks, `notify` and tool
    /// server were pointed at when it started. A user this daemon can no longer serve (under the
    /// supervisor, one dropped from `agent_tool_sockets:`) still gets the terminal back: the refusal
    /// is logged, naming the setting, and that agent's reports are not answered.
    pub(super) async fn serve_agent_tool_socket_of(&self, os_user: &str, session_id: &str) {
        if let Err(e) = self
            .agent_tool_sockets
            .ensure_bound(os_user, &self.tddy_data_dir)
            .await
        {
            log::warn!(
                "re-attaching session {session_id}: its agent's reports go unanswered, {os_user}'s \
                 agent-tool socket cannot be served: {e:#}"
            );
        }
    }
}

impl TmuxMainReattach {
    /// Re-attach `session_id`'s main on behalf of `os_user`, and say how that ended. The outcome is
    /// announced on the manager, as every re-attach attempt is.
    pub(super) async fn attach(&self, os_user: &str, session_id: &str) -> ReattachOutcome {
        let gate = Arc::clone(
            self.in_flight
                .lock()
                .await
                .entry(session_id.to_string())
                .or_default(),
        );
        let attached = {
            let _one_at_a_time = gate.lock().await;
            if self.manager.get(session_id).await.is_some() {
                ReattachOutcome::AlreadyAttached
            } else {
                self.serve_agent_tool_socket_of(os_user, session_id).await;
                tddy_agent_launch::claude_cli_tmux::reattach_recorded_main(
                    &self.manager,
                    Some(&*self.coding_agents),
                    &self.tddy_data_dir,
                    os_user,
                    session_id,
                )
                .await
            }
        };
        self.manager.announce_reattach(session_id, attached);
        // The last access out forgets the session's gate (every clone is taken under this lock, so
        // a count of two — the map's and this one — means nobody else holds or waits on it).
        let mut in_flight = self.in_flight.lock().await;
        if Arc::strong_count(&gate) == 2 {
            in_flight.remove(session_id);
        }
        attached
    }
}

#[async_trait]
impl MainReattach for TmuxMainReattach {
    async fn reattach_main(&self, os_user: &str, session_id: &str) -> bool {
        matches!(
            self.attach(os_user, session_id).await,
            ReattachOutcome::Reattached | ReattachOutcome::AlreadyAttached
        )
    }
}

/// The control lease, held by [`CliSessionManager`].
///
/// The lease is the manager's rather than this adapter's because a session's *deletion* clears it,
/// and deletion runs nowhere near an RPC handler.
struct CliManagerTerminalControl {
    manager: Arc<CliSessionManager>,
}

impl CliManagerTerminalControl {
    fn new(manager: Arc<CliSessionManager>) -> Self {
        CliManagerTerminalControl { manager }
    }
}

#[async_trait]
impl TerminalControl for CliManagerTerminalControl {
    async fn claim(&self, session_id: &str, screen_id: &str, steal: bool) -> ControlClaim {
        self.manager
            .claim_control(session_id, screen_id, steal)
            .await
    }

    async fn verify(&self, session_id: &str, control_token: &str) -> bool {
        self.manager.verify_control(session_id, control_token).await
    }

    async fn holder_screen_id(&self, session_id: &str) -> Option<String> {
        self.manager
            .current_control(session_id)
            .await
            .map(|lease| lease.holder_screen_id)
    }

    async fn release(&self, session_id: &str, control_token: &str) -> bool {
        self.manager
            .release_control(session_id, control_token)
            .await
    }

    fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.manager.subscribe_control()
    }
}

/// The terminals of a session, spawned as tasks by [`CliSessionManager`].
struct CliManagerTerminalRoster {
    manager: Arc<CliSessionManager>,
    /// The coding agents an agent tab's binary is resolved from: the spec's, never daemon.yaml's.
    coding_agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    /// Where session directories live — a stopped session's worktree is read from its record.
    tddy_data_dir: PathBuf,
    /// The host's tmux, which a tmux tab's session name is checked against before anything spawns.
    tmux: Arc<dyn TmuxHost>,
    /// Where a jailed tab's jail is registered, and what starts it (`#session-liveness` 5/8; agent
    /// tabs 7/8); `None` on a roster that starts no jailed tab.
    tab_jails: Option<JailedTabs>,
    /// What resolves a target session's target for a tab opened "On target" (`#session-liveness`
    /// 6/8); `None` on a roster that starts no target tab.
    target_shell: Option<Arc<dyn tddy_agent_launch::tab_target_shell::TabTargetShell>>,
    /// The closes running on this host: no tab of a session being closed is started.
    session_closes: Arc<super::svc_close_session::SessionCloses>,
}

/// The tab-jail registry and the spawn a roster starts jailed shell tabs through.
struct JailedTabs {
    jails: Arc<tddy_daemon_sandbox::tab_jails::TabJails>,
    spawn: Arc<dyn tddy_daemon_sandbox::tab_jails::TabJailSpawn>,
}

impl CliManagerTerminalRoster {
    fn new(
        manager: Arc<CliSessionManager>,
        coding_agents: Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
        tddy_data_dir: PathBuf,
        tmux: Arc<dyn TmuxHost>,
        session_closes: Arc<super::svc_close_session::SessionCloses>,
    ) -> Self {
        CliManagerTerminalRoster {
            manager,
            coding_agents,
            tddy_data_dir,
            tmux,
            tab_jails: None,
            target_shell: None,
            session_closes,
        }
    }

    /// Start jailed shell tabs through `spawn`, registering their jails in `jails`.
    fn with_tab_jails(
        mut self,
        jails: Arc<tddy_daemon_sandbox::tab_jails::TabJails>,
        spawn: Arc<dyn tddy_daemon_sandbox::tab_jails::TabJailSpawn>,
    ) -> Self {
        self.tab_jails = Some(JailedTabs { jails, spawn });
        self
    }

    /// Resolve the target of a tab opened "On target" through `shells`.
    fn with_tab_target_shell(
        mut self,
        shells: Arc<dyn tddy_agent_launch::tab_target_shell::TabTargetShell>,
    ) -> Self {
        self.target_shell = Some(shells);
        self
    }
}

#[async_trait]
impl TerminalRoster for CliManagerTerminalRoster {
    /// A started terminal runs in the same worktree the session's agent does: the live main
    /// terminal's when the agent runs, and the one the session's record names when it is stopped —
    /// but for a target session, whose worktree is on its target, the session's directory here.
    async fn start(&self, session_id: &str, launch: &TerminalLaunch) -> Result<String, Status> {
        // A tab started while the session is being closed would run in a worktree the close is
        // about to remove (docs/ft/daemon/session-liveness.md § Closing a session).
        self.session_closes.refuse_while_closing(session_id)?;
        if launch.kind == tddy_terminal_rpc::service::TerminalKind::SandboxedShell {
            return self.start_jailed_shell_tab(session_id, launch).await;
        }
        if let tddy_terminal_rpc::service::TerminalKind::SandboxedAgent { agent } = &launch.kind {
            return self.start_jailed_agent_tab(session_id, agent, launch).await;
        }
        if launch.kind == tddy_terminal_rpc::service::TerminalKind::TargetShell {
            return self.start_target_shell_tab(session_id, launch).await;
        }
        if let tddy_terminal_rpc::service::TerminalKind::TargetAgent { agent } = &launch.kind {
            return self.start_target_agent_tab(session_id, agent, launch).await;
        }
        if let Some(started) = self.start_target_tmux_tab(session_id, launch).await {
            return started;
        }
        let worktree = match self.manager.get(session_id).await {
            Some(main) => main.worktree_path.clone(),
            // A target session's worktree is on its target: a tab "On this host" runs in the
            // session's own directory here (`#session-liveness` 6/8).
            None if target_tabs::records_a_target(&self.tddy_data_dir, session_id) => {
                self.target_sessions_host_directory(session_id)?
            }
            None => recorded_worktree::recorded_worktree(&self.tddy_data_dir, session_id)?,
        };
        let tab = self.start_tab(session_id, worktree, launch).await?;
        self.record_started(session_id, &tab).await?;
        Ok(tab.handle.terminal_id.clone())
    }

    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool {
        self.close_tab(session_id, terminal_id, None).await
    }

    async fn stop_as(&self, session_id: &str, terminal_id: &str, subject: &str) -> bool {
        self.close_tab(session_id, terminal_id, Some(subject)).await
    }

    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor> {
        let mut terminals: Vec<TerminalDescriptor> = self
            .manager
            .list_terminals(session_id)
            .await
            .iter()
            .map(|handle| TerminalDescriptor {
                terminal_id: handle.terminal_id.clone(),
                kind: handle.kind.clone(),
                pid: handle.pid,
                tmux_session: handle.tmux_session.clone(),
            })
            .collect();
        terminals.extend(self.listed_jailed_tabs(session_id).await);
        terminals
    }

    async fn launch_chains(
        &self,
        session_id: &str,
    ) -> Result<
        std::collections::HashMap<
            String,
            Vec<tddy_terminal_rpc::proto::terminal_session::LaunchNode>,
        >,
        Status,
    > {
        self.recorded_chains(session_id)
    }
}
