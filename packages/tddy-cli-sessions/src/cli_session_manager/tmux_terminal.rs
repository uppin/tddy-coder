//! Any terminal of a session as a tmux client — the per-terminal generalisation of the main-only
//! [`CliSessionManager::attach_main_to_tmux`] and [`CliSessionManager::reattach_main`]
//! (`#session-liveness` 2/4).

use std::path::PathBuf;
use std::sync::Arc;

use super::{pty_handle, CliSessionManager};

impl CliSessionManager {
    /// Attach `session_id`'s terminal `terminal_id` to the tmux session `tmux_session`: a tmux
    /// client of kind `kind` run in `worktree` as `os_user`, recorded with
    /// `tmux_session = Some(tmux_session)`.
    ///
    /// The agent itself runs in tmux; this is only the window onto it, so the client ending does
    /// not end the agent. A client that ends while its tmux session lives leaves a detach watcher
    /// for `(session_id, terminal_id)`, which keeps the session live until the tmux session ends.
    pub async fn attach_terminal_to_tmux(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        os_user: &str,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        // TODO(session-liveness 2/4): implemented in the green step.
        todo!(
            "attach terminal {terminal_id} ({kind}) of {session_id} in {} to tmux session \
             {tmux_session:?} as {os_user}",
            worktree.display()
        )
    }

    /// Re-attach `session_id`'s terminal `terminal_id` to the live tmux session `tmux_session`: what
    /// [`attach_terminal_to_tmux`](Self::attach_terminal_to_tmux) does, after stopping the
    /// terminal's detach watcher. A terminal already attached to `tmux_session` is returned as it
    /// is, so a second re-attach adds no second client. The agent is not touched.
    pub async fn reattach_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        os_user: &str,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        // TODO(session-liveness 2/4): implemented in the green step.
        todo!(
            "re-attach terminal {terminal_id} ({kind}) of {session_id} in {} to tmux session \
             {tmux_session:?} as {os_user}",
            worktree.display()
        )
    }
}
