//! A target session's **agent tab on the target** (`#session-liveness` 8/8): the session's target
//! resolved through the roster's [`TabTargetShell`](tddy_agent_launch::tab_target_shell::TabTargetShell)
//! as the caller's subject, the tab settled by the target variant of the agent-tab rule
//! ([`tddy_agent_launch::restore_tabs::new_target_agent_tab`]: the agent's bare command, its
//! conversation minted on the target), launched there in the session's worktree
//! ([`tddy_agent_launch::remote_agent_launch::launch_agent_tab_on_target`]) as a local tab terminal,
//! and recorded `[target, agent]` in the session's roster. A target that cannot be resolved or
//! reached fails the start with why, and nothing is recorded.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-agent-tab.md

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
        // TODO(target-agent-tab): implement
        let _ = (
            &self.target_shell,
            &self.coding_agents,
            session_id,
            agent,
            launch,
        );
        todo!("start an agent tab on the session's target")
    }
}
