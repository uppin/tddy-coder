//! A jailed **agent** tab's jail (`#session-liveness` 7/8): the daemon's [`TabJailSpawn::spawn_agent`].
//!
//! Split in two so the half that decides what the jail is can be read without a kernel sandbox:
//!
//! - [`TabJailAgentPlanner::plan`] builds the jail as the session main's is built
//!   ([`LaunchSessions::launch_jailed`](crate::launch_ports::LaunchSessions::launch_jailed)): the agent's grants lowered from its tool's closure
//!   ([`lower_tool_jail`]), its argv rendered from its spec's `run:` template in the **tab's**
//!   conversation, and the runner's exec plan written — every path the tab's own
//!   ([`TabJailLaunch::paths`]). A persistent agent home the main's jail also uses is used as is.
//!   It writes **none** of the session's records: the role `launch_jailed` writes before its spawn
//!   (`record_role_of_session_type_before_spawn`) and the usage context it writes once the home is
//!   known (`record_usage_context`) are the main's.
//! - [`TabJailSpawn::spawn_agent`] spawns the runner from that plan, waits for it to be ready and
//!   bridges its terminal, sharing those pieces of `launch_jailed` rather than growing it.
//!
//! [`lower_tool_jail`]: tddy_daemon_sandbox::tool_jail::lower_tool_jail
//! [`TabJailSpawn::spawn_agent`]: tddy_daemon_sandbox::tab_jails::TabJailSpawn::spawn_agent
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-agent-tab.md

use std::path::Path;

use tddy_daemon_sandbox::tab_jails::{TabJailAgent, TabJailLaunch};
use tddy_daemon_sandbox::tool_jail::ToolJailGrants;
use tddy_rpc::Status;
use tddy_sandbox_runner::ExecPlan;

use crate::spec_launch::HostCodingAgents;

/// What a jailed agent tab's runner is started from.
#[derive(Debug, Clone)]
pub struct TabJailAgentPlan {
    /// What the runner execs: the agent's argv in the tab's conversation, its env, its slot files.
    pub exec_plan: ExecPlan,
    /// What the tab's jail is granted: the main's for the same agent, over the tab's own scratch.
    pub grants: ToolJailGrants,
}

/// Plans a jailed agent tab's jail from the coding agents this host installed.
pub struct TabJailAgentPlanner<'a> {
    /// What a tab's harness is resolved to: its spec, its tool and its closure.
    pub coding_agents: &'a HostCodingAgents,
    /// `<tddyhome>`: where persistent agent homes live.
    pub tddy_home: &'a Path,
    /// The host user's home, which the agent's `from_host` files are relative to.
    pub host_home: &'a Path,
}

impl TabJailAgentPlanner<'_> {
    /// The jail of `agent` for the tab `launch` describes: granted what the session main's jail is
    /// for the same agent, its tool links and scratch under `launch.paths.scratch_dir`, its exec plan
    /// written to `launch.paths.exec_plan` — `agent.conversation_id` started, or resumed when
    /// `agent.resume`. Writes none of the session's records. A harness no coding agent runs is
    /// `INVALID_ARGUMENT` naming it, and nothing is written.
    pub fn plan(
        &self,
        launch: &TabJailLaunch,
        agent: &TabJailAgent,
    ) -> Result<TabJailAgentPlan, Status> {
        // TODO(sandbox-agent-tab): implement
        let _ = (
            self.coding_agents,
            self.tddy_home,
            self.host_home,
            launch,
            agent,
        );
        todo!("plan a jailed agent tab's jail")
    }
}

#[cfg(test)]
#[path = "tab_jail_agent_tests.rs"]
mod tests;
