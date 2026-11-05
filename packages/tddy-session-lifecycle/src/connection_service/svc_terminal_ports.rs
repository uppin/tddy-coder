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
use tddy_cli_sessions::tmux::{
    ensure_reachable, tmux_attach_argv, tmux_binary, tmux_new_session_argv, tmux_session_name,
};
use tddy_core::cli_harness::{cli_harness, CliLaunch};
use tddy_daemon_kernel::privilege_drop::EffectiveIdentity;
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{
    ControlChange, ControlClaim, MainReattach, TerminalControl, TerminalDescriptor, TerminalKind,
    TerminalLaunch, TerminalRoster, TerminalSessionPorts, TmuxHost,
};
use tokio::sync::broadcast;

use super::DaemonSessionHost;
use crate::cli_session_manager::CliSessionManager;
use crate::config::DaemonConfig;
use crate::terminal_session_adapter::DaemonTerminalSessionStore;

mod recorded_worktree;
mod tmux_host;

impl DaemonSessionHost {
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
            Arc::clone(&self.claude_cli_manager),
            Arc::clone(&self.sandbox_manager),
        )
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
            &self.claude_cli_manager,
        )))
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
        let tmux: Arc<dyn TmuxHost> = Arc::new(tmux_host::HostTmux);
        TerminalSessionPorts {
            github_users: self.user_resolver.clone(),
            os_users: Arc::new(move |github_user: &str| config.os_user_for_github(github_user)),
            terminals: Arc::new(self.terminal_sessions()),
            control: self.terminal_control(),
            roster: Arc::new(CliManagerTerminalRoster::new(
                Arc::clone(&self.claude_cli_manager),
                self.config.clone(),
                self.tddy_data_dir.clone(),
                Arc::clone(&tmux),
            )),
            recorded_history: Arc::new(
                crate::session_dir_recorded_history::SessionDirRecordedHistory::new(
                    self.tddy_data_dir.clone(),
                ),
            ),
            main_reattach: Arc::new(TmuxMainReattach::new(
                Arc::clone(&self.claude_cli_manager),
                self.coding_agents.clone(),
                self.tddy_data_dir.clone(),
            )),
            tmux,
            initial_frame_bytes: tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        }
    }
}

/// Re-attaches the main terminal of a tmux-run session whose agent outlived its client or its
/// daemon, on the first access that finds no main registered.
///
/// Single-flight per session: concurrent first accesses wait on one lock, and the second finds the
/// main the first attached.
struct TmuxMainReattach {
    manager: Arc<CliSessionManager>,
    /// The coding agents this daemon installed: what a record is re-attached as.
    coding_agents: Option<Arc<tddy_agent_launch::spec_launch::HostCodingAgents>>,
    tddy_data_dir: PathBuf,
    in_flight: tokio::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl TmuxMainReattach {
    fn new(
        manager: Arc<CliSessionManager>,
        coding_agents: Option<Arc<tddy_agent_launch::spec_launch::HostCodingAgents>>,
        tddy_data_dir: PathBuf,
    ) -> Self {
        TmuxMainReattach {
            manager,
            coding_agents,
            tddy_data_dir,
            in_flight: tokio::sync::Mutex::default(),
        }
    }
}

#[async_trait]
impl MainReattach for TmuxMainReattach {
    async fn reattach_main(&self, os_user: &str, session_id: &str) -> bool {
        let gate = Arc::clone(
            self.in_flight
                .lock()
                .await
                .entry(session_id.to_string())
                .or_default(),
        );
        let attached = {
            let _one_at_a_time = gate.lock().await;
            self.manager.get(session_id).await.is_some()
                || tddy_agent_launch::claude_cli_tmux::reattach_recorded_main(
                    &self.manager,
                    self.coding_agents.as_deref(),
                    &self.tddy_data_dir,
                    os_user,
                    session_id,
                )
                .await
        };
        // The last access out forgets the session's gate (every clone is taken under this lock, so
        // a count of two — the map's and this one — means nobody else holds or waits on it).
        let mut in_flight = self.in_flight.lock().await;
        if Arc::strong_count(&gate) == 2 {
            in_flight.remove(session_id);
        }
        attached
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

    fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.manager.subscribe_control()
    }
}

/// The terminals of a session, spawned as tasks by [`CliSessionManager`].
struct CliManagerTerminalRoster {
    manager: Arc<CliSessionManager>,
    config: DaemonConfig,
    /// Where session directories live — a stopped session's worktree is read from its record.
    tddy_data_dir: PathBuf,
    /// The host's tmux, which a tmux tab's session name is checked against before anything spawns.
    tmux: Arc<dyn TmuxHost>,
}

impl CliManagerTerminalRoster {
    fn new(
        manager: Arc<CliSessionManager>,
        config: DaemonConfig,
        tddy_data_dir: PathBuf,
        tmux: Arc<dyn TmuxHost>,
    ) -> Self {
        CliManagerTerminalRoster {
            manager,
            config,
            tddy_data_dir,
            tmux,
        }
    }
}

#[async_trait]
impl TerminalRoster for CliManagerTerminalRoster {
    /// A started terminal runs in the same worktree the session's agent does: the live main
    /// terminal's when the agent runs, and the one the session's record names when it is stopped.
    async fn start(&self, session_id: &str, launch: &TerminalLaunch) -> Result<String, Status> {
        let worktree = match self.manager.get(session_id).await {
            Some(main) => main.worktree_path.clone(),
            None => recorded_worktree::recorded_worktree(&self.tddy_data_dir, session_id)?,
        };
        let handle = match &launch.kind {
            TerminalKind::Bash => self
                .manager
                .start_terminal(session_id, worktree, &launch.login_shell)
                .await
                .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?,
            TerminalKind::Agent(kind) => {
                let harness = cli_harness(kind).ok_or_else(|| {
                    Status::invalid_argument(format!("unknown terminal kind {kind:?}"))
                })?;
                let binary = self.config.cli_harness_binary(
                    harness.session_type,
                    harness.default_binary,
                    harness.binary_env,
                );
                // A tab is a conversation of its own: a harness told a session id (Claude Code)
                // gets a fresh one, never the session's.
                let conversation_id = uuid::Uuid::now_v7().to_string();
                let argv = (harness.argv)(&CliLaunch {
                    binary: &binary,
                    session_id: &conversation_id,
                    ..CliLaunch::default()
                });
                self.manager
                    .start_agent_terminal(session_id, worktree, harness.session_type, argv)
                    .await
                    .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?
            }
            TerminalKind::Tmux { session } => {
                // Reachability first: a daemon that cannot become the session's user refuses with
                // the named error, and never starts the tab as the daemon user.
                ensure_reachable(&launch.os_user, &EffectiveIdentity::current())?;
                // Checked before anything spawns: a client for a name tmux does not hold would
                // attach to nothing and exit, leaving a dead tab behind.
                let sessions = self.tmux.list(&launch.os_user).await?;
                if !sessions.iter().any(|listed| listed.name == *session) {
                    return Err(Status::not_found(format!(
                        "no tmux session named {session:?} on this host for this user"
                    )));
                }
                let binary = tmux_binary();
                self.manager
                    .start_tmux_terminal(
                        session_id,
                        worktree,
                        &launch.os_user,
                        tmux_attach_argv(&binary, session),
                        session,
                    )
                    .await
                    .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?
            }
            TerminalKind::TmuxNew { session } => {
                // The name is validated before anything else, so a refused name never reaches tmux.
                let name = tmux_session_name(session)?;
                ensure_reachable(&launch.os_user, &EffectiveIdentity::current())?;
                let binary = tmux_binary();
                let argv = tmux_new_session_argv(&binary, &name, &worktree);
                self.manager
                    .start_tmux_terminal(session_id, worktree, &launch.os_user, argv, &name)
                    .await
                    .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?
            }
        };
        Ok(handle.terminal_id.clone())
    }

    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool {
        self.manager.stop_terminal(session_id, terminal_id).await
    }

    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor> {
        self.manager
            .list_terminals(session_id)
            .await
            .iter()
            .map(|handle| TerminalDescriptor {
                terminal_id: handle.terminal_id.clone(),
                kind: handle.kind.clone(),
                pid: handle.pid,
                tmux_session: handle.tmux_session.clone(),
            })
            .collect()
    }
}
