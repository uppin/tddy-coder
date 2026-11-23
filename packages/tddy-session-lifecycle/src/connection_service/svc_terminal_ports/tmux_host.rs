//! [`SessionTmux`]: the daemon's [`TmuxHost`] — the daemon host's tmux, and for a session that runs
//! on a target, the target login's tmux (`#target-tmux` 2/2).
//!
//! The host's tmux helpers live in [`tddy_cli_sessions::tmux`] — moved down so the agent launcher
//! can use them too (`#tmux-agent` 1/3).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_agent_launch::remote_agent_launch::{list_target_tmux_sessions, tab_launch_status};
use tddy_agent_launch::tab_target_shell::TabTargetShell;
use tddy_cli_sessions::tmux::list_tmux_sessions;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;
use tddy_terminal_rpc::{TmuxHost, TmuxListing, TmuxSession};

use super::target_tabs::records_a_target;

/// [`TmuxHost`] over the daemon host's tmux, run as the asking OS user; a session recorded on a
/// target lists its target's tmux instead, never the host's.
pub(super) struct SessionTmux {
    /// Where session directories live: a session's record says whether it runs on a target.
    pub tddy_data_dir: PathBuf,
    /// What resolves a target session's target, as the asking subject.
    pub target_shell: Arc<dyn TabTargetShell>,
}

#[async_trait]
impl TmuxHost for SessionTmux {
    async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status> {
        let os_user = os_user.to_string();
        tokio::task::spawn_blocking(move || list_tmux_sessions(&os_user))
            .await
            .map_err(|e| Status::internal(format!("tmux listing task failed: {e}")))?
    }

    async fn list_for_session(
        &self,
        listing: &TmuxListing<'_>,
    ) -> Result<Vec<TmuxSession>, Status> {
        if !records_a_target(&self.tddy_data_dir, listing.session_id) {
            return self.list(listing.os_user).await;
        }
        let session_dir = unified_session_dir_path(&self.tddy_data_dir, listing.session_id);
        let target = self.target_shell.target_of(&session_dir, listing.subject)?;
        list_target_tmux_sessions(target.shell.as_ref(), &target.worktree)
            .await
            .map_err(|error| tab_launch_status(&error))
    }
}
