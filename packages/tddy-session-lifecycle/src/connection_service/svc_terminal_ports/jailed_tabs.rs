//! A sandboxed session's **jailed shell tab** (`#session-liveness` 5/8): started in a jail of its
//! own through the roster's [`TabJailSpawn`](tddy_daemon_sandbox::tab_jails::TabJailSpawn),
//! registered in [`TabJails`](tddy_daemon_sandbox::tab_jails::TabJails) under
//! `(session_id, terminal_id)`, and recorded `[sandbox, shell]` in the session's roster. A session
//! that is not sandboxed refuses one `FAILED_PRECONDITION`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md

use tddy_core::tab_roster::{LaunchNode, TabEntry};
use tddy_daemon_sandbox::tab_jails::TabJailLaunch;
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{TerminalDescriptor, TerminalLaunch};

use super::recorded_worktree::recorded_worktree;
use super::CliManagerTerminalRoster;

impl CliManagerTerminalRoster {
    /// The jailed shell tabs of `session_id` whose jail runs: each is a shell, as `bash` lists it.
    pub(super) async fn listed_jailed_tabs(&self, session_id: &str) -> Vec<TerminalDescriptor> {
        let Some(tabs) = &self.tab_jails else {
            return Vec::new();
        };
        let mut listed = Vec::new();
        for terminal_id in tabs.jails.live(session_id).await {
            if let Some(jail) = tabs.jails.get(session_id, &terminal_id).await {
                listed.push(TerminalDescriptor {
                    terminal_id,
                    kind: "bash".to_string(),
                    pid: jail.pid,
                    tmux_session: None,
                });
            }
        }
        listed
    }

    /// Start a shell tab of `session_id` in a jail of its own over the session's worktree and
    /// grants, record it `[sandbox, shell]`, and return its fresh terminal id. A jail that cannot
    /// start fails the start with why, and nothing is recorded.
    pub(super) async fn start_jailed_shell_tab(
        &self,
        session_id: &str,
        _launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let Some(tabs) = &self.tab_jails else {
            return Err(Status::failed_precondition(
                "this daemon starts no tab jails",
            ));
        };
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
        let jail = TabJailLaunch::for_tab(session_id, &terminal_id, &session_dir, &worktree)?;
        tabs.jails.start(&*tabs.spawn, jail).await?;
        let chain = vec![LaunchNode::sandbox(), LaunchNode::shell()];
        if let Err(error) = self
            .manager
            .tab_rosters()
            .record(&session_dir, TabEntry::new(&terminal_id, chain))
            .await
        {
            // An unrecorded tab could not be restored, and nothing would know to stop its jail.
            tabs.jails.stop(session_id, &terminal_id).await;
            return Err(Status::internal(format!(
                "failed to record the tab: {error}"
            )));
        }
        Ok(terminal_id)
    }
}
