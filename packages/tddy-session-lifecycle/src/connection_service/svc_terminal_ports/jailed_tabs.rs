//! A sandboxed session's **jailed shell and agent tabs** (`#session-liveness` 5/8, 7/8): each
//! started in a jail of its own through the roster's [`TabJailSpawn`](tddy_daemon_sandbox::tab_jails::TabJailSpawn),
//! registered in [`TabJails`](tddy_daemon_sandbox::tab_jails::TabJails) under
//! `(session_id, terminal_id)`, and recorded `[sandbox, shell]` or `[sandbox, agent]` in the session's roster. A session
//! that is not sandboxed refuses one `FAILED_PRECONDITION`.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A sandboxed session's tabs)

use std::path::{Path, PathBuf};

use tddy_agent_launch::restore_tabs::{agent_launch_node, NewAgentTab};
use tddy_core::tab_roster::{LaunchNode, TabEntry};
use tddy_daemon_sandbox::tab_jails::{TabJailAgent, TabJailLaunch, TabJailOwner};
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{TerminalDescriptor, TerminalLaunch};

use super::recorded_worktree::recorded_worktree;
use super::{CliManagerTerminalRoster, JailedTabs};

impl CliManagerTerminalRoster {
    /// The jailed shell tabs of `session_id` whose jail runs: each is a shell, as `bash` lists it.
    pub(super) async fn listed_jailed_tabs(&self, session_id: &str) -> Vec<TerminalDescriptor> {
        let Some(tabs) = &self.tab_jails else {
            return Vec::new();
        };
        let mut listed = Vec::new();
        for terminal_id in tabs.jails.live(session_id).await {
            if let Some(jail) = tabs.jails.get(session_id, &terminal_id).await {
                // A jailed agent tab is listed as its agent, a jailed shell as `bash`.
                let kind = tabs
                    .jails
                    .agent_harness(session_id, &terminal_id)
                    .await
                    .unwrap_or_else(|| "bash".to_string());
                listed.push(TerminalDescriptor {
                    terminal_id,
                    kind,
                    pid: jail.pid,
                    tmux_session: None,
                });
            }
        }
        listed
    }

    /// Start a shell tab of `session_id` in a jail of its own over the session's worktree and
    /// grants, run for the session's OS user and its login shell, record it `[sandbox, shell]`, and return its fresh terminal id. A jail that cannot
    /// start fails the start with why, and nothing is recorded.
    pub(super) async fn start_jailed_shell_tab(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let tabs = self.jailed_tabs()?;
        let (session_dir, jail) = self.jailed_tab_launch(session_id, launch).await?;
        let terminal_id = jail.terminal_id.clone();
        tabs.jails.start(&*tabs.spawn, jail).await?;
        self.record_jailed_tab(
            tabs,
            session_id,
            &session_dir,
            TabEntry::new(
                &terminal_id,
                vec![LaunchNode::sandbox(), LaunchNode::shell()],
            ),
        )
        .await?;
        Ok(terminal_id)
    }

    /// The registry and spawn port of this daemon's tab jails; `FAILED_PRECONDITION` without them.
    fn jailed_tabs(&self) -> Result<&JailedTabs, Status> {
        self.tab_jails
            .as_ref()
            .ok_or_else(|| Status::failed_precondition("this daemon starts no tab jails"))
    }

    /// The session's directory and the launch of a fresh tab's jail in it, for the session's sandbox
    /// and its worktree, run for `launch`'s OS user; `FAILED_PRECONDITION` for a session that is not
    /// sandboxed.
    async fn jailed_tab_launch(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
    ) -> Result<(PathBuf, TabJailLaunch), Status> {
        let session_dir = self.session_dir(session_id)?;
        let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|error| {
            Status::internal(format!(
                "failed to read the record of session {session_id}: {error}"
            ))
        })?;
        if metadata.sandbox != Some(true) {
            return Err(Status::failed_precondition(format!(
                "session {session_id} is not sandboxed, so it has no jail to open a tab in"
            )));
        }
        let worktree = recorded_worktree(&self.tddy_data_dir, session_id)?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let jail = TabJailLaunch::for_tab(
            session_id,
            &terminal_id,
            &session_dir,
            &worktree,
            TabJailOwner {
                os_user: &launch.os_user,
                login_shell: &launch.login_shell,
                subject: &launch.subject,
            },
        )?;
        Ok((session_dir, jail))
    }

    /// Record `entry` in the session's roster. An unrecorded tab could not be restored and nothing
    /// would know to stop its jail, so a tab that cannot be recorded has its jail stopped.
    async fn record_jailed_tab(
        &self,
        tabs: &JailedTabs,
        session_id: &str,
        session_dir: &Path,
        entry: TabEntry,
    ) -> Result<(), Status> {
        let terminal_id = entry.terminal_id.clone();
        if let Err(error) = self.manager.tab_rosters().record(session_dir, entry).await {
            tabs.jails.stop(session_id, &terminal_id).await;
            return Err(Status::internal(format!(
                "failed to record the tab: {error}"
            )));
        }
        Ok(())
    }

    /// Start an agent tab of `session_id` running the harness `agent` in a jail of its own over the
    /// session's worktree and the agent's grants (`#session-liveness` 7/8): its conversation settled
    /// by the agent-tab rule (`tddy_agent_launch::restore_tabs::new_agent_tab`), registered as an
    /// agent jail so it keeps the session live, recorded `[sandbox, agent]`; return its fresh
    /// terminal id. Writes none of the session's records. A session that is not sandboxed refuses
    /// it `FAILED_PRECONDITION`; a jail that cannot start fails the start with why, and nothing is
    /// recorded.
    pub(super) async fn start_jailed_agent_tab(
        &self,
        session_id: &str,
        agent: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let tabs = self.jailed_tabs()?;
        let (session_dir, jail) = self.jailed_tab_launch(session_id, launch).await?;
        let terminal_id = jail.terminal_id.clone();
        let NewAgentTab {
            conversation_id,
            resumable,
            ..
        } = self.new_agent_tab(agent, &jail.worktree).await?;
        let jailed_agent = TabJailAgent {
            harness: agent.to_string(),
            conversation_id: conversation_id.clone(),
            resume: false,
        };
        tabs.jails
            .start_agent(&*tabs.spawn, jail, jailed_agent)
            .await?;
        self.record_jailed_tab(
            tabs,
            session_id,
            &session_dir,
            TabEntry::new(
                &terminal_id,
                vec![
                    LaunchNode::sandbox(),
                    agent_launch_node(agent, &conversation_id, resumable),
                ],
            ),
        )
        .await?;
        Ok(terminal_id)
    }
}
