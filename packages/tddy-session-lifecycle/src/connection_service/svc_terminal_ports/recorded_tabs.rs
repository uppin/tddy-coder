//! What the terminal roster does beyond spawning a tab: record the launch chain every tab starts
//! with in its session's `tabs.yaml`, forget a tab that is closed, run an agent wrapped in tmux, and
//! answer the chains a listing reports (`#session-liveness` 2/4).
//!
//! The file has one writer, [`tddy_cli_sessions::tab_roster_store::TabRosterStore`], held by the
//! manager. A tab that exits on its own is not forgotten here: Resume brings it back.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::tmux::{
    agent_tab_tmux_session_name, ensure_reachable, kill_tmux_session, start_agent_tmux_session,
    tmux_attach_argv, tmux_binary, tmux_new_session_argv, tmux_session_name,
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
        let internal =
            |e: anyhow::Error| Status::internal(format!("failed to start terminal: {e}"));
        let (handle, chain) = match &launch.kind {
            TerminalKind::Bash => {
                let handle = self
                    .manager
                    .start_terminal(session_id, worktree, &launch.login_shell)
                    .await
                    .map_err(internal)?;
                (handle, vec![LaunchNode::shell()])
            }
            TerminalKind::Agent(kind) => {
                let (argv, conversation_id) = self.agent_tab_argv(kind)?;
                let handle = self
                    .manager
                    .start_agent_terminal(session_id, worktree, kind, argv)
                    .await
                    .map_err(internal)?;
                (handle, vec![LaunchNode::agent(kind, &conversation_id)])
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
                let handle = self
                    .manager
                    .start_tmux_terminal(
                        session_id,
                        worktree,
                        &launch.os_user,
                        tmux_attach_argv(&tmux_binary(), session),
                        session,
                    )
                    .await
                    .map_err(internal)?;
                (
                    handle,
                    vec![LaunchNode::tmux_attach(session, &launch.os_user)],
                )
            }
            TerminalKind::TmuxNew { session } => {
                // The name is validated before anything else, so a refused name never reaches tmux.
                let name = tmux_session_name(session)?;
                ensure_reachable(&launch.os_user, &EffectiveIdentity::current())?;
                let argv = tmux_new_session_argv(&tmux_binary(), &name, &worktree);
                let handle = self
                    .manager
                    .start_tmux_terminal(session_id, worktree, &launch.os_user, argv, &name)
                    .await
                    .map_err(internal)?;
                (
                    handle,
                    vec![LaunchNode::tmux_attach(&name, &launch.os_user)],
                )
            }
            TerminalKind::TmuxWrappedAgent { agent } => {
                return self
                    .start_agent_in_tmux(session_id, worktree, agent, &launch.os_user)
                    .await
            }
        };
        Ok(StartedTab {
            handle,
            launch: chain,
        })
    }

    /// The session's directory, refusing an id that is not one path segment.
    fn session_dir(&self, session_id: &str) -> Result<PathBuf, Status> {
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(unified_session_dir_path(&self.tddy_data_dir, session_id))
    }

    /// The argv an agent tab of `kind` runs, and the conversation it holds: the spec's `run.argv`
    /// rendered with a conversation id of its own — a tab is never the session's conversation.
    pub(super) fn agent_tab_argv(&self, kind: &str) -> Result<(Vec<String>, String), Status> {
        let spec = self
            .coding_agents
            .agents()
            .for_session_type(kind)
            .ok_or_else(|| Status::invalid_argument(format!("unknown terminal kind {kind:?}")))?;
        // The binary is the agent spec's, resolved on this host (`#agent-tools 19/19`): its
        // `binary.env`, then the host's search, refusing what is found nowhere.
        let binary = self
            .coding_agents
            .resolve(&spec.id, None)
            .map_err(|e| Status::failed_precondition(e.to_string()))?
            .tool
            .binary;
        let conversation_id = uuid::Uuid::now_v7().to_string();
        let argv = spec
            .render_run_argv(
                &binary,
                &[
                    ("session_id", conversation_id.as_str()),
                    ("chat_id", conversation_id.as_str()),
                ],
                &[],
            )
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok((argv, conversation_id))
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
        let (argv, conversation_id) = self.agent_tab_argv(kind)?;
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
                    LaunchNode::agent(kind, &conversation_id),
                ],
            }),
            Err(error) => {
                // The agent must not outlive a start that failed: nothing records it.
                kill_tmux_session_blocking(os_user, &name).await;
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
            self.end(session_id, terminal_id, &tab.launch).await;
        }
        recorded
    }

    /// Stop `terminal_id`, and with it the tmux session its chain says wraps its agent.
    async fn end(&self, session_id: &str, terminal_id: &str, launch: &[LaunchNode]) {
        // The tmux session first: its client then ends with the agent, so no detach watcher holds
        // the session live for an agent nobody can reach.
        if let Some(wrapper) = launch.iter().find(|node| node.kind == TMUX_KIND) {
            if let (Some(name), Some(os_user)) = (
                wrapper.property(tddy_core::tab_roster::SESSION_PROPERTY),
                wrapper.property(tddy_core::tab_roster::OS_USER_PROPERTY),
            ) {
                kill_tmux_session_blocking(os_user, name).await;
            }
        }
        self.manager.stop_terminal(session_id, terminal_id).await;
    }

    /// Close `terminal_id`: end it, and remove it from the roster. `false` when the session has no
    /// such terminal.
    pub(super) async fn close_tab(&self, session_id: &str, terminal_id: &str) -> bool {
        if self
            .manager
            .get_terminal(session_id, terminal_id)
            .await
            .is_none()
        {
            return false;
        }
        let recorded = self.recorded_chain(session_id, terminal_id);
        self.end(session_id, terminal_id, &recorded).await;
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
        true
    }

    /// The chain the roster records for `terminal_id`; empty when it records none (or the roster
    /// cannot be read, which is logged — closing the tab does not depend on it).
    fn recorded_chain(&self, session_id: &str, terminal_id: &str) -> Vec<LaunchNode> {
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

fn wire_node(node: LaunchNode) -> WireLaunchNode {
    WireLaunchNode {
        kind: node.kind,
        properties: node.properties.into_iter().collect(),
    }
}

/// Kill tmux session `name` of `os_user` on the blocking pool. The caller is already failing or
/// closing, so a kill that fails is logged rather than returned.
async fn kill_tmux_session_blocking(os_user: &str, name: &str) {
    let (user, session) = (os_user.to_string(), name.to_string());
    match tokio::task::spawn_blocking(move || kill_tmux_session(&user, &session)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            log::warn!("could not end tmux session {name:?} of {os_user}: {error}")
        }
        Err(error) => {
            log::warn!("the end of tmux session {name:?} of {os_user} did not finish: {error}")
        }
    }
}
