//! What the terminal roster does beyond spawning a tab: record the launch chain every tab starts
//! with in its session's `tabs.yaml`, forget a tab that is closed, run an agent wrapped in tmux, and
//! answer the chains a listing reports (`#session-liveness` 2/4).
//!
//! The file has one writer, [`tddy_cli_sessions::tab_roster_store::TabRosterStore`], held by the
//! manager. A tab that exits on its own is not forgotten here: Resume brings it back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_agent_launch::restore_tabs::{agent_launch_node, new_agent_tab, NewAgentTab};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::tmux::{
    agent_tab_tmux_session_name, ensure_reachable, kill_tmux_session, kill_tmux_session_logged,
    start_agent_tmux_session, tmux_attach_argv, tmux_binary, tmux_new_session_argv,
    tmux_session_name,
};
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::tab_roster::{read_tab_roster, LaunchNode, TabEntry, TMUX_KIND};
use tddy_daemon_kernel::privilege_drop::EffectiveIdentity;
use tddy_rpc::Status;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode as WireLaunchNode;
use tddy_terminal_rpc::service::{TerminalKind, TerminalLaunch};

use super::CliManagerTerminalRoster;

/// A tab started by the roster, with the chain it was launched through (outermost first).
pub(super) struct StartedTab {
    pub handle: Arc<PtyHandle>,
    pub launch: Vec<LaunchNode>,
}

impl CliManagerTerminalRoster {
    /// Start the terminal `launch` describes in `worktree`, with the chain it was launched through.
    pub(super) async fn start_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        launch: &TerminalLaunch,
    ) -> Result<StartedTab, Status> {
        match &launch.kind {
            TerminalKind::Bash => {
                self.start_shell_tab(session_id, worktree, &launch.login_shell)
                    .await
            }
            TerminalKind::Agent(kind) => self.start_agent_tab(session_id, worktree, kind).await,
            TerminalKind::Tmux { session } => {
                self.start_tmux_attach_tab(session_id, worktree, &launch.os_user, session)
                    .await
            }
            TerminalKind::TmuxNew { session } => {
                self.start_tmux_new_tab(session_id, worktree, &launch.os_user, session)
                    .await
            }
            TerminalKind::TmuxWrappedAgent { agent } => {
                self.start_agent_in_tmux(session_id, worktree, agent, &launch.os_user)
                    .await
            }
            // A jailed shell has no host PTY: `TerminalRoster::start` hands it to its jail.
            TerminalKind::SandboxedShell => Err(Status::internal(
                "a sandboxed shell starts in a jail of its own, not on the host",
            )),
            // A jailed agent has no host PTY either: `TerminalRoster::start` hands it to its jail.
            TerminalKind::SandboxedAgent { .. } => Err(Status::internal(
                "a sandboxed agent starts in a jail of its own, not on the host",
            )),
            // A target shell runs on the target: `TerminalRoster::start` hands it to its transport.
            TerminalKind::TargetShell => Err(Status::internal(
                "a target shell starts on the session's target, not in a worktree on this host",
            )),
            // An agent on a target runs there too: `TerminalRoster::start` hands it to its transport.
            TerminalKind::TargetAgent { .. } => Err(Status::internal(
                "an agent on a target starts on the session's target, not in a worktree on this host",
            )),
            // A tmux tab on a target runs there too (`#target-tmux` 2/2).
            TerminalKind::TargetTmux { .. }
            | TerminalKind::TargetTmuxNew { .. }
            | TerminalKind::TargetTmuxAgent { .. } => Err(Status::internal(
                "a tmux tab on a target starts on the session's target, not in a worktree on this host",
            )),
        }
    }

    /// A login shell in the session's worktree.
    async fn start_shell_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        login_shell: &str,
    ) -> Result<StartedTab, Status> {
        let handle = self
            .manager
            .start_terminal(session_id, worktree, login_shell)
            .await
            .map_err(failed_to_start)?;
        Ok(StartedTab {
            handle,
            launch: vec![LaunchNode::shell()],
        })
    }

    /// An agent of `kind` in a conversation of its own.
    async fn start_agent_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        kind: &str,
    ) -> Result<StartedTab, Status> {
        let tab = self.new_agent_tab(kind, &worktree).await?;
        let handle = self
            .manager
            .start_agent_terminal(session_id, worktree, kind, tab.argv)
            .await
            .map_err(failed_to_start)?;
        Ok(StartedTab {
            handle,
            launch: vec![agent_launch_node(kind, &tab.conversation_id, tab.resumable)],
        })
    }

    /// A client attached to the user's own tmux session `session`.
    async fn start_tmux_attach_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        os_user: &str,
        session: &str,
    ) -> Result<StartedTab, Status> {
        // Reachability first: a daemon that cannot become the session's user refuses with the named
        // error, and never starts the tab as the daemon user.
        ensure_reachable(os_user, &EffectiveIdentity::current())?;
        // Checked before anything spawns: a client for a name tmux does not hold would attach to
        // nothing and exit, leaving a dead tab behind.
        let sessions = self.tmux.list(os_user).await?;
        if !sessions.iter().any(|listed| listed.name == session) {
            return Err(Status::not_found(format!(
                "no tmux session named {session:?} on this host for this user"
            )));
        }
        let argv = tmux_attach_argv(&tmux_binary(), session);
        self.start_tmux_client_tab(session_id, worktree, os_user, argv, session)
            .await
    }

    /// A client on a new tmux session `session`, started in the worktree.
    async fn start_tmux_new_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        os_user: &str,
        session: &str,
    ) -> Result<StartedTab, Status> {
        // The name is validated before anything else, so a refused name never reaches tmux.
        let name = tmux_session_name(session)?;
        ensure_reachable(os_user, &EffectiveIdentity::current())?;
        let argv = tmux_new_session_argv(&tmux_binary(), &name, &worktree);
        self.start_tmux_client_tab(session_id, worktree, os_user, argv, &name)
            .await
    }

    /// Run `argv` as a tmux client of session `name`. Both ways of getting a client on the user's
    /// own tmux are recorded as attaching to `name`: the session is there either way.
    async fn start_tmux_client_tab(
        &self,
        session_id: &str,
        worktree: PathBuf,
        os_user: &str,
        argv: Vec<String>,
        name: &str,
    ) -> Result<StartedTab, Status> {
        let handle = self
            .manager
            .start_tmux_terminal(session_id, worktree, os_user, argv, name)
            .await
            .map_err(failed_to_start)?;
        Ok(StartedTab {
            handle,
            launch: vec![LaunchNode::tmux_attach(name, os_user)],
        })
    }

    /// The session's directory, refusing an id that is not one path segment.
    pub(super) fn session_dir(&self, session_id: &str) -> Result<PathBuf, Status> {
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(unified_session_dir_path(&self.tddy_data_dir, session_id))
    }

    /// A new agent tab of `kind` in `worktree`: what it runs and the conversation it holds. The
    /// restore (`#session-liveness` 4/4) shares the rule — see [`new_agent_tab`].
    pub(super) async fn new_agent_tab(
        &self,
        kind: &str,
        worktree: &Path,
    ) -> Result<NewAgentTab, Status> {
        new_agent_tab(&self.coding_agents, kind, worktree).await
    }

    /// Run agent tab `kind` in a tmux session of its own, `tddy-<session_id>-<terminal_id>`, as
    /// `os_user`, and attach a client tab to it.
    pub(super) async fn start_agent_in_tmux(
        &self,
        session_id: &str,
        worktree: PathBuf,
        kind: &str,
        os_user: &str,
    ) -> Result<StartedTab, Status> {
        // The same reachability check the tmux tab makes: a daemon that cannot become the session's
        // user refuses with the named error, and never starts the agent as the daemon user.
        ensure_reachable(os_user, &EffectiveIdentity::current())?;
        let NewAgentTab {
            argv,
            conversation_id,
            resumable,
        } = self.new_agent_tab(kind, &worktree).await?;
        let session_dir = self.session_dir(session_id)?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let name = agent_tab_tmux_session_name(session_id, &terminal_id);
        {
            let (os_user, name, worktree) = (os_user.to_string(), name.clone(), worktree.clone());
            tokio::task::spawn_blocking(move || {
                start_agent_tmux_session(
                    &os_user,
                    &name,
                    &worktree,
                    &session_dir,
                    &[],
                    &argv,
                    false,
                )
            })
            .await
            .map_err(|e| Status::internal(format!("tmux task did not finish: {e}")))?
            .map_err(|e| Status::internal(format!("failed to spawn {kind}: {e}")))?;
        }
        match self
            .manager
            .attach_terminal_to_tmux(session_id, &terminal_id, kind, worktree, os_user, &name)
            .await
        {
            Ok(handle) => Ok(StartedTab {
                handle,
                launch: vec![
                    LaunchNode::tmux(&name, os_user),
                    agent_launch_node(kind, &conversation_id, resumable),
                ],
            }),
            Err(error) => {
                // The agent must not outlive a start that failed: nothing records it.
                kill_tmux_session_logged(os_user, &name, "a start that failed").await;
                Err(Status::internal(format!(
                    "failed to attach a terminal to tmux session {name:?}: {error}"
                )))
            }
        }
    }

    /// Record `tab` in its session's roster. A tab that cannot be recorded is stopped, and the
    /// start fails: an unrecorded tab could not be restored, and nothing would know to stop it.
    pub(super) async fn record_started(
        &self,
        session_id: &str,
        tab: &StartedTab,
    ) -> Result<(), Status> {
        let terminal_id = tab.handle.terminal_id.as_str();
        let recorded = match self.session_dir(session_id) {
            Ok(dir) => self
                .manager
                .tab_rosters()
                .record(&dir, TabEntry::new(terminal_id, tab.launch.clone()))
                .await
                .map_err(|e| Status::internal(format!("failed to record the tab: {e}"))),
            Err(refusal) => Err(refusal),
        };
        if recorded.is_err() {
            self.end(session_id, terminal_id, &tab.launch, None).await;
        }
        recorded
    }

    /// Stop `terminal_id`, and with it the tmux session its chain says wraps its agent. The tab
    /// closes either way: a tmux session that could not be ended is logged.
    async fn end(
        &self,
        session_id: &str,
        terminal_id: &str,
        launch: &[LaunchNode],
        subject: Option<&str>,
    ) {
        if let Err(error) = self.end_wrapping_tmux(session_id, launch, subject).await {
            log::warn!("terminal {terminal_id} of session {session_id} closed, but {error}");
        }
        self.manager.stop_terminal(session_id, terminal_id).await;
    }

    /// End tab `terminal_id` of `session_id` as its session ends (a close or a delete): its jail,
    /// the tmux session its recorded chain says wraps its agent — here, or on the target as
    /// `subject` — and its terminal. Unlike a closed tab it stays in the roster, which the session
    /// keeps; and a tmux session that could not be ended is the error, since the session's end must
    /// know of a survivor.
    ///
    /// `tmux` is what the tab's recorded chain says wraps its agent, as the session's end read it
    /// with the roster — never re-read here, where a roster that could not be read would end nothing.
    pub(super) async fn end_with_its_session(
        &self,
        session_id: &str,
        terminal_id: &str,
        tmux: Option<&WrappingTmux>,
        subject: Option<&str>,
    ) -> Result<(), String> {
        if let Some(tabs) = &self.tab_jails {
            tabs.jails.stop(session_id, terminal_id).await;
        }
        let ended = self.end_tmux(session_id, tmux.cloned(), subject).await;
        self.manager.stop_terminal(session_id, terminal_id).await;
        ended
    }

    /// End the tmux session `launch` records wrapping a tab's agent: on the session's target for a
    /// chain that starts `target`, here as the recorded user otherwise. A chain with no `tmux` node
    /// (a shell, an agent, a tmux attach) ends nothing: the tmux session a person keeps outlives a
    /// tab attached to it. The tmux session goes first: its client then ends with the agent, so no
    /// detach watcher holds the session live for an agent nobody can reach.
    async fn end_wrapping_tmux(
        &self,
        session_id: &str,
        launch: &[LaunchNode],
        subject: Option<&str>,
    ) -> Result<(), String> {
        self.end_tmux(session_id, wrapping_tmux(launch), subject)
            .await
    }

    /// End `tmux`, the tmux session wrapping a tab's agent: on the session's target, or here as its
    /// user. No tmux session ends nothing.
    async fn end_tmux(
        &self,
        session_id: &str,
        tmux: Option<WrappingTmux>,
        subject: Option<&str>,
    ) -> Result<(), String> {
        match tmux {
            None => Ok(()),
            Some(WrappingTmux::OnTarget { name }) => {
                self.end_target_tmux_named(session_id, &name, subject).await
            }
            Some(WrappingTmux::Here { name, os_user }) => {
                let (user, session) = (os_user.clone(), name.clone());
                tokio::task::spawn_blocking(move || kill_tmux_session(&user, &session))
                    .await
                    .map_err(|e| {
                        format!("the kill of its tmux session {name:?} did not finish: {e}")
                    })?
                    .map_err(|e| format!("its tmux session {name:?} could not be ended: {e}"))
            }
        }
    }

    /// Close `terminal_id`: end it, and remove it from the roster. `false` when the session has no
    /// such terminal.
    pub(super) async fn close_tab(
        &self,
        session_id: &str,
        terminal_id: &str,
        subject: Option<&str>,
    ) -> bool {
        let jailed = match &self.tab_jails {
            Some(tabs) => tabs.jails.stop(session_id, terminal_id).await,
            None => false,
        };
        if !jailed {
            if self
                .manager
                .get_terminal(session_id, terminal_id)
                .await
                .is_none()
            {
                return false;
            }
            let recorded = self.recorded_chain(session_id, terminal_id);
            self.end(session_id, terminal_id, &recorded, subject).await;
        }
        self.forget_closed_tab(session_id, terminal_id).await;
        true
    }

    /// Remove the closed `terminal_id` from its session's roster; a roster that cannot be written
    /// is logged, the tab being closed either way.
    async fn forget_closed_tab(&self, session_id: &str, terminal_id: &str) {
        match self.session_dir(session_id) {
            Ok(dir) => {
                if let Err(error) = self.manager.tab_rosters().forget(&dir, terminal_id).await {
                    log::error!(
                        "closed terminal {terminal_id} of session {session_id} is still in its tab \
                         roster: {error:#}"
                    );
                }
            }
            Err(refusal) => log::error!(
                "closed terminal {terminal_id} of session {session_id} cannot be forgotten: {}",
                refusal.message()
            ),
        }
    }

    /// The chain the roster records for `terminal_id`; empty when it records none (or the roster
    /// cannot be read, which is logged — closing the tab does not depend on it).
    pub(super) fn recorded_chain(&self, session_id: &str, terminal_id: &str) -> Vec<LaunchNode> {
        let read = self
            .session_dir(session_id)
            .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))
            .and_then(|dir| read_tab_roster(&dir));
        match read {
            Ok(roster) => roster
                .tabs
                .into_iter()
                .find(|tab| tab.terminal_id == terminal_id)
                .map(|tab| tab.launch)
                .unwrap_or_default(),
            Err(error) => {
                log::warn!(
                    "the tab roster of session {session_id} cannot be read, so closing \
                     {terminal_id} cannot tell whether it runs in tmux: {error:#}"
                );
                Vec::new()
            }
        }
    }

    /// The chain each recorded tab of `session_id` was launched through, as the wire carries them.
    pub(super) fn recorded_chains(
        &self,
        session_id: &str,
    ) -> Result<HashMap<String, Vec<WireLaunchNode>>, Status> {
        let dir = self.session_dir(session_id)?;
        let roster = read_tab_roster(&dir)
            .map_err(|e| Status::internal(format!("failed to read the tab roster: {e:#}")))?;
        Ok(roster
            .tabs
            .into_iter()
            .map(|tab| {
                (
                    tab.terminal_id,
                    tab.launch.into_iter().map(wire_node).collect(),
                )
            })
            .collect())
    }
}

fn failed_to_start(error: anyhow::Error) -> Status {
    Status::internal(format!("failed to start terminal: {error}"))
}

fn wire_node(node: LaunchNode) -> WireLaunchNode {
    WireLaunchNode {
        kind: node.kind,
        properties: node.properties.into_iter().collect(),
    }
}

/// The tmux session a tab's chain records wrapping its agent, and where it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WrappingTmux {
    /// On this host, run as `os_user`.
    Here { name: String, os_user: String },
    /// On the session's target.
    OnTarget { name: String },
}

/// The tmux session `launch` records wrapping an agent (its `tmux` node), here or — for a chain
/// that starts `target` — on the session's target. `None` for a chain with no `tmux` node, or a
/// node that does not name its session (and, here, its user).
pub(super) fn wrapping_tmux(launch: &[LaunchNode]) -> Option<WrappingTmux> {
    let wrapper = launch.iter().find(|node| node.kind == TMUX_KIND)?;
    let name = wrapper
        .property(tddy_core::tab_roster::SESSION_PROPERTY)?
        .to_string();
    if launch.first().map(|node| node.kind.as_str()) == Some(tddy_core::tab_roster::TARGET_KIND) {
        return Some(WrappingTmux::OnTarget { name });
    }
    let os_user = wrapper
        .property(tddy_core::tab_roster::OS_USER_PROPERTY)?
        .to_string();
    Some(WrappingTmux::Here { name, os_user })
}
