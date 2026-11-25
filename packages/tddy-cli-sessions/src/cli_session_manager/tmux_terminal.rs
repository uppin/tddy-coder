//! Any terminal of a session as a tmux client — the per-terminal generalisation of the main-only
//! [`CliSessionManager::attach_main_to_tmux`] and [`CliSessionManager::reattach_main`]
//! (`#session-liveness` 2/4).

use std::path::PathBuf;
use std::sync::Arc;

use super::{pty_handle, CliSessionManager};
use crate::tmux_location::{HostTmux, SharedTmuxLocation};

impl CliSessionManager {
    /// Attach `session_id`'s terminal `terminal_id` to the tmux session `tmux_session` on the
    /// host of `os_user`: what [`Self::attach_terminal_at_location`] does with a [`HostTmux`].
    pub async fn attach_terminal_to_tmux(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        os_user: &str,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.attach_terminal_at_location(
            session_id,
            terminal_id,
            kind,
            worktree,
            Arc::new(HostTmux::new(os_user)),
            tmux_session,
        )
        .await
    }

    /// Attach `session_id`'s terminal `terminal_id` to the tmux session `tmux_session` at
    /// `location`: a client built by the location, recorded with `tmux_session`.
    ///
    /// The agent itself runs in tmux; this is only the window onto it, so the client ending does
    /// not end the agent. A client that ends while its tmux session lives leaves a detach watcher
    /// for `(session_id, terminal_id)`, which keeps the session live until the tmux session ends.
    pub async fn attach_terminal_at_location(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        location: SharedTmuxLocation,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let client = location
            .attach_client(tmux_session)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "cannot attach to tmux session {tmux_session:?} at {}: {error}",
                    location.describe()
                )
            })?;
        self.spawn_tool_on(
            session_id,
            terminal_id,
            kind,
            worktree,
            "",
            client.argv,
            client.env,
            tddy_pty::PtyEnvBase::Inherited,
            client.exit_code_in_output,
            client.run_as.as_deref(),
            None,
            Some(tmux_session),
            Some(location),
        )
        .await
    }

    /// Re-attach `session_id`'s terminal `terminal_id` to the live tmux session `tmux_session` on
    /// the host of `os_user`: what [`Self::reattach_terminal_at_location`] does with a [`HostTmux`].
    pub async fn reattach_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        os_user: &str,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.reattach_terminal_at_location(
            session_id,
            terminal_id,
            kind,
            worktree,
            Arc::new(HostTmux::new(os_user)),
            tmux_session,
        )
        .await
    }

    /// Re-attach `session_id`'s terminal `terminal_id` to the live tmux session `tmux_session` at
    /// `location`, after stopping the terminal's detach watcher. A terminal already attached to
    /// `tmux_session` is returned as it is, so a second re-attach adds no second client. The agent
    /// is not touched.
    pub async fn reattach_terminal_at_location(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree: PathBuf,
        location: SharedTmuxLocation,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        if let Some(attached) = self
            .get_terminal(session_id, terminal_id)
            .await
            .filter(|terminal| terminal.tmux_session.as_deref() == Some(tmux_session))
        {
            return Ok(attached);
        }
        self.detached_watchers
            .write()
            .await
            .stop(session_id, terminal_id);
        self.attach_terminal_at_location(
            session_id,
            terminal_id,
            kind,
            worktree,
            location,
            tmux_session,
        )
        .await
    }
}
