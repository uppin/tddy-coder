//! The daemon's [`TabJailSpawn`]: a jailed shell tab's jail, launched from the session's own jail
//! closure (`#session-liveness` 5/8).
//!
//! The jail is lowered as the session's main jail is ([`lower_tool_jail`] over the session agent's
//! closure, [`build_tool_jail_plan`], [`spawn_sandbox_plan`], the host relay), so it has the same
//! worktree mount, egress policy and tool relay — but its runner execs the session user's login
//! shell, and every file it writes is the tab's own ([`TabJailLaunch::paths`]). It shares the
//! extracted pieces of [`LaunchSessions::launch_jailed`] rather than growing it.
//!
//! [`lower_tool_jail`]: tddy_daemon_sandbox::tool_jail::lower_tool_jail
//! [`build_tool_jail_plan`]: tddy_daemon_sandbox::tool_jail::build_tool_jail_plan
//! [`spawn_sandbox_plan`]: tddy_daemon_sandbox::sandbox_session::spawn_sandbox_plan
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md

use std::sync::Arc;

use async_trait::async_trait;
use tddy_daemon_sandbox::sandbox_session::SandboxSessionState;
use tddy_daemon_sandbox::tab_jails::{TabJailLaunch, TabJailSpawn};
use tddy_rpc::Status;

use crate::launch_ports::LaunchSessions;

impl LaunchSessions {
    /// What starts a tab's jail: the double a test gave the host, or this topic's own launch.
    pub fn tab_jail_spawn(&self) -> Arc<dyn TabJailSpawn> {
        match &self.tab_jail_spawn {
            Some(spawn) => Arc::clone(spawn),
            None => Arc::new(self.clone()),
        }
    }
}

#[async_trait]
impl TabJailSpawn for LaunchSessions {
    async fn spawn_shell(
        &self,
        launch: &TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        // TODO(sandbox-shell-tab): implement
        let _ = launch;
        todo!("launch a shell tab's jail")
    }
}
