//! A session's main terminal as a tmux client (`#tmux-agent` 1/3).

use std::path::PathBuf;
use std::sync::Arc;

use crate::tmux::{tmux_attach_argv, tmux_binary};

use super::{pty_handle, CliSessionManager, MAIN_TERMINAL_ID};

impl CliSessionManager {
    /// Attach `session_id`'s **main terminal** (`MAIN_TERMINAL_ID`) to the tmux session `name`: a
    /// tmux client of kind `kind` run in `worktree` as `os_user`, recorded with
    /// `tmux_session = Some(name)`.
    ///
    /// The client is an ordinary main: it carries the exit hook that finishes its recording. The
    /// caller registers the recording (`record_main_capture`) once, as for every other main, so it
    /// is not registered twice here.
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
        let argv = tmux_attach_argv(&tmux_binary(), name);
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            kind,
            worktree,
            "",
            argv,
            Vec::new(),
            Some(os_user),
            None,
            Some(name),
        )
        .await
    }
}
