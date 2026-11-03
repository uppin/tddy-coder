//! A session's main terminal as a tmux client (`#tmux-agent` 1/3).

use std::path::PathBuf;
use std::sync::Arc;

use super::{pty_handle, CliSessionManager};

impl CliSessionManager {
    /// Attach `session_id`'s **main terminal** (`MAIN_TERMINAL_ID`) to the tmux session `name`: a
    /// tmux client of kind `kind` run in `worktree` as `os_user`, recorded with
    /// `tmux_session = Some(name)` and carrying the main terminal's recording hook, so its
    /// history outlives the process like any other main.
    ///
    /// The agent itself runs in tmux (see [`crate::tmux::start_agent_tmux_session`]); this is only
    /// the window onto it, so the client ending does not end the agent.
    pub async fn attach_main_to_tmux(
        &self,
        session_id: &str,
        worktree: PathBuf,
        os_user: &str,
        kind: &str,
        name: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        // TODO(claude-in-tmux): implement
        let _ = (self, session_id, worktree, os_user, kind, name);
        unimplemented!("TODO(claude-in-tmux): attach_main_to_tmux")
    }
}
