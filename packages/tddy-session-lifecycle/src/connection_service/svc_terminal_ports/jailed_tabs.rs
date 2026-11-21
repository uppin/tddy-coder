//! A sandboxed session's **jailed shell tab** (`#session-liveness` 5/8): started in a jail of its
//! own through the roster's [`TabJailSpawn`](tddy_daemon_sandbox::tab_jails::TabJailSpawn),
//! registered in [`TabJails`](tddy_daemon_sandbox::tab_jails::TabJails) under
//! `(session_id, terminal_id)`, and recorded `[sandbox, shell]` in the session's roster. A session
//! that is not sandboxed refuses one `FAILED_PRECONDITION`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md

use tddy_rpc::Status;
use tddy_terminal_rpc::service::TerminalLaunch;

use super::CliManagerTerminalRoster;

impl CliManagerTerminalRoster {
    /// Start a shell tab of `session_id` in a jail of its own over the session's worktree and
    /// grants, record it `[sandbox, shell]`, and return its fresh terminal id. A jail that cannot
    /// start fails the start with why, and nothing is recorded.
    pub(super) async fn start_jailed_shell_tab(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        // TODO(sandbox-shell-tab): implement
        let _ = (
            self.tab_jails
                .as_ref()
                .map(|tabs| (&tabs.jails, &tabs.spawn)),
            session_id,
            launch,
        );
        todo!("start a jailed shell tab")
    }
}
