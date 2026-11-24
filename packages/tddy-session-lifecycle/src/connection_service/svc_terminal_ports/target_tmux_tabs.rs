//! A target session's **tmux tabs on the target** (`#target-tmux` 2/2): a tab attached to one of the
//! target login's tmux sessions, a tab on a new tmux session there, and an agent run in a tmux
//! session of its own there — each a client of `tmux new-session -A` run through the target's login
//! shell in the session's worktree ([`tddy_agent_launch::remote_agent_launch::target_tmux_tab_command`]),
//! recorded `[target, tmux-attach]` or `[target, tmux, agent]`. Closing an agent-in-tmux tab kills its
//! tmux session on the target, as closing one on this host kills its host tmux session.
//!
//! The target resolves through the roster's
//! [`TabTargetShell`](tddy_agent_launch::tab_target_shell::TabTargetShell) as the caller's subject;
//! a failed launch is reported by the mapping every target tab shares
//! ([`tddy_agent_launch::remote_agent_launch::tab_launch_status`]), and nothing is recorded.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md (n2)

use tddy_agent_launch::remote_agent_launch::{
    end_target_tmux_session_named, launch_agent_tab_on_target, launch_tmux_tab_on_target,
    list_target_tmux_sessions, tab_launch_status, target_tmux_session_lives, TargetTab,
};
use tddy_agent_launch::restore_tabs::{agent_launch_node, new_target_agent_tab, NewTargetAgentTab};
use tddy_agent_launch::tab_target_shell::TabTarget;
use tddy_cli_sessions::tmux::{agent_tab_tmux_session_name, tmux_session_name};
use tddy_core::tab_roster::{LaunchNode, TabEntry};
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{TerminalKind, TerminalLaunch};

use super::CliManagerTerminalRoster;

impl CliManagerTerminalRoster {
    /// Start the tmux tab `launch` describes on `session_id`'s target, when it is one — `None` for
    /// any other kind.
    pub(super) async fn start_target_tmux_tab(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
    ) -> Option<Result<String, Status>> {
        Some(match &launch.kind {
            TerminalKind::TargetTmux { session } => {
                self.start_target_tmux_client(session_id, launch, session, true)
                    .await
            }
            TerminalKind::TargetTmuxNew { session } => match tmux_session_name(session) {
                Ok(name) => {
                    self.start_target_tmux_client(session_id, launch, &name, false)
                        .await
                }
                Err(refusal) => Err(refusal),
            },
            TerminalKind::TargetTmuxAgent { agent } => {
                self.start_target_tmux_agent(session_id, agent, launch)
                    .await
            }
            _ => return None,
        })
    }

    /// The session's target, resolved as the caller.
    fn target_for(&self, session_id: &str, subject: &str) -> Result<TabTarget, Status> {
        let Some(shells) = &self.target_shell else {
            return Err(Status::failed_precondition(
                "this daemon starts no tabs on a target",
            ));
        };
        shells.target_of(&self.session_dir(session_id)?, subject)
    }

    /// A client of the tmux session `name` on the target: one the target must already list when
    /// `must_exist`, made in the worktree there otherwise. Recorded `[target, tmux-attach]`.
    async fn start_target_tmux_client(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
        name: &str,
        must_exist: bool,
    ) -> Result<String, Status> {
        let target = self.target_for(session_id, &launch.subject)?;
        if must_exist {
            // Checked before anything spawns: a client of a name the target does not hold would
            // make a session the caller never asked for.
            let listed = list_target_tmux_sessions(target.shell.as_ref(), &target.worktree)
                .await
                .map_err(|error| tab_launch_status(&error))?;
            if !listed.iter().any(|session| session.name == name) {
                return Err(Status::not_found(format!(
                    "no tmux session named {name:?} on the session's target"
                )));
            }
        }
        let session_dir = self.session_dir(session_id)?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
        launch_tmux_tab_on_target(
            &self.manager,
            session_id,
            &terminal_id,
            TargetTab {
                shell: target.shell,
                worktree: &target.worktree,
                session_dir: &session_dir,
            },
            name,
        )
        .await
        .map_err(|error| tab_launch_status(&error))?;
        let chain = vec![
            LaunchNode::target(),
            LaunchNode::tmux_attach(name, &launch.os_user),
        ];
        self.record_target_tab(session_id, &terminal_id, chain)
            .await?;
        Ok(terminal_id)
    }

    /// The agent of `agent` run in a tmux session of its own on the target,
    /// `tddy-<session>-<terminal>`, the tab a client of it. Recorded `[target, tmux, agent]`.
    async fn start_target_tmux_agent(
        &self,
        session_id: &str,
        agent: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let target = self.target_for(session_id, &launch.subject)?;
        let NewTargetAgentTab {
            agent: mut tab,
            conversation_id,
            resumable,
        } = new_target_agent_tab(&self.coding_agents, agent, &target).await?;
        let session_dir = self.session_dir(session_id)?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let name = agent_tab_tmux_session_name(session_id, &terminal_id);
        tab.tmux_session = Some(name.clone());
        launch_agent_tab_on_target(
            &self.manager,
            session_id,
            &terminal_id,
            TargetTab {
                shell: target.shell,
                worktree: &target.worktree,
                session_dir: &session_dir,
            },
            &tab,
        )
        .await
        .map_err(|error| tab_launch_status(&error))?;
        let chain = vec![
            LaunchNode::target(),
            LaunchNode::tmux(&name, &launch.os_user),
            agent_launch_node(agent, &conversation_id, resumable),
        ];
        self.record_target_tab(session_id, &terminal_id, chain)
            .await?;
        Ok(terminal_id)
    }

    /// Record a started target tab; one that cannot be recorded is stopped and the start fails.
    async fn record_target_tab(
        &self,
        session_id: &str,
        terminal_id: &str,
        chain: Vec<LaunchNode>,
    ) -> Result<(), Status> {
        let session_dir = self.session_dir(session_id)?;
        if let Err(error) = self
            .manager
            .tab_rosters()
            .record(&session_dir, TabEntry::new(terminal_id, chain))
            .await
        {
            // An unrecorded tab could not be restored, and nothing would know to end it.
            self.manager.stop_terminal(session_id, terminal_id).await;
            return Err(Status::internal(format!(
                "failed to record the tab: {error}"
            )));
        }
        Ok(())
    }

    /// End the tmux session `name` wrapping a target tab's agent, on `session_id`'s target, as
    /// `subject`. A tab ended with no caller cannot reach the target, and says so; so does a target
    /// that cannot be resolved or reached, or a tmux there that refuses the kill.
    pub(super) async fn end_target_tmux_named(
        &self,
        session_id: &str,
        name: &str,
        subject: Option<&str>,
    ) -> Result<(), String> {
        let Some(subject) = subject else {
            return Err(format!(
                "its tmux session {name:?} on the target is left running: no caller to reach the \
                 target as"
            ));
        };
        let ended = match self.target_for(session_id, subject) {
            Ok(target) => end_target_tmux_session_named(target.shell.as_ref(), name)
                .await
                .map_err(|error| error.to_string()),
            Err(refusal) => Err(refusal.message().to_string()),
        };
        ended.map_err(|error| {
            format!("its tmux session {name:?} on the target could not be ended: {error}")
        })
    }

    /// Whether the tmux session `name` still lives on `session_id`'s target, asked as `subject`.
    pub(super) async fn target_tmux_lives(
        &self,
        session_id: &str,
        name: &str,
        subject: Option<&str>,
    ) -> Result<bool, String> {
        let subject =
            subject.ok_or_else(|| "no caller to reach the session's target as".to_string())?;
        let target = self
            .target_for(session_id, subject)
            .map_err(|refusal| refusal.message().to_string())?;
        target_tmux_session_lives(target.shell.as_ref(), name)
            .await
            .map_err(|error| error.to_string())
    }
}
