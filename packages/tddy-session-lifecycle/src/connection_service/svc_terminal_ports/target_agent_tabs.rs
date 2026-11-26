//! A target session's **agent tab on the target** (`#session-liveness` 8/8): the session's target
//! resolved through the roster's [`TabTargetShell`](tddy_agent_launch::tab_target_shell::TabTargetShell)
//! as the caller's subject, the tab settled by the target variant of the agent-tab rule
//! ([`tddy_agent_launch::restore_tabs::new_target_agent_tab`]: the agent's bare command, its
//! conversation minted on the target), launched there in the session's worktree
//! ([`tddy_agent_launch::remote_agent_launch::launch_agent_tab_on_target`]) as a local tab terminal,
//! and recorded `[target, agent]` in the session's roster. A target that cannot be resolved fails the
//! start `FAILED_PRECONDITION`; a failed launch — an unreachable target, an agent the target lacks, a
//! local terminal that cannot spawn — is reported by the mapping every target tab shares
//! ([`tddy_agent_launch::remote_agent_launch::tab_launch_status`]); nothing is recorded either way.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use tddy_agent_launch::remote_agent_launch::{
    launch_agent_tab_on_target, tab_launch_status, TargetTab,
};
use tddy_agent_launch::restore_tabs::{agent_launch_node, new_target_agent_tab, NewTargetAgentTab};
use tddy_core::tab_roster::{LaunchNode, TabEntry};
use tddy_rpc::Status;
use tddy_terminal_rpc::service::TerminalLaunch;

use super::CliManagerTerminalRoster;

impl CliManagerTerminalRoster {
    /// Start an agent tab of `session_id` running the harness `agent` on the session's target, in
    /// its worktree there, record it `[target, agent]`, and return its fresh terminal id.
    pub(super) async fn start_target_agent_tab(
        &self,
        session_id: &str,
        agent: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let Some(shells) = &self.target_shell else {
            return Err(Status::failed_precondition(
                "this daemon starts no tabs on a target",
            ));
        };
        let session_dir = self.session_dir(session_id)?;
        let target = shells.target_of(&session_dir, &launch.subject)?;
        let NewTargetAgentTab {
            agent: tab,
            conversation_id,
            resumable,
        } = new_target_agent_tab(&self.coding_agents, agent, &target).await?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
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
            None,
        )
        .await
        .map_err(|error| tab_launch_status(&error))?;
        let chain = vec![
            LaunchNode::target(),
            agent_launch_node(agent, &conversation_id, resumable),
        ];
        if let Err(error) = self
            .manager
            .tab_rosters()
            .record(&session_dir, TabEntry::new(&terminal_id, chain))
            .await
        {
            // An unrecorded tab could not be restored, and nothing would know to end it.
            self.manager.stop_terminal(session_id, &terminal_id).await;
            return Err(Status::internal(format!(
                "failed to record the tab: {error}"
            )));
        }
        Ok(terminal_id)
    }
}
