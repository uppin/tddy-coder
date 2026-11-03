//! Re-attaching a session's main terminal to the tmux session its agent runs in, and telling a
//! tmux client ending from the agent ending (`#tmux-agent` 2/3).

use std::path::PathBuf;
use std::sync::Arc;

use super::{pty_handle, CliSessionManager};

/// Why a main terminal's process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainExit {
    /// The tmux *client* ended (a detach, a shutdown) and the agent's tmux session is alive: the
    /// terminal entry is dropped, but the session's workflow and exposure stay.
    ClientOnly,
    /// The agent ended — a PTY-run main, or a tmux-run main whose tmux session is gone: today's
    /// main-exit cleanup.
    AgentEnded,
}

/// Classify the end of a main terminal. `tmux_session` is the terminal's recorded tmux session
/// (`None` for a PTY-run main); `tmux_session_alive` is the liveness probe, injected so the
/// decision is testable without a tmux server. A terminal without a tmux session never probes.
pub fn classify_main_exit(
    tmux_session: Option<&str>,
    tmux_session_alive: impl FnOnce(&str) -> bool,
) -> MainExit {
    match tmux_session {
        Some(name) if tmux_session_alive(name) => MainExit::ClientOnly,
        _ => MainExit::AgentEnded,
    }
}

impl CliSessionManager {
    /// Re-attach `session_id`'s main terminal to the live tmux session `name`: what
    /// [`attach_main_to_tmux`](Self::attach_main_to_tmux) does. Returns the main handle.
    ///
    /// A main already attached to `name` is returned as it is, so a second resume of a live session
    /// does not add a second client. The agent is not touched.
    ///
    /// The caller registers the main recording, as it does after a start (the recording path is the
    /// caller's to name, not this manager's). The exposure record is not re-registered here: a
    /// client's exit in this daemon keeps it (see [`classify_main_exit`]), and a restarted daemon has
    /// no record to re-register from.
    pub async fn reattach_main(
        &self,
        session_id: &str,
        os_user: &str,
        name: &str,
        worktree: PathBuf,
        kind: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        if let Some(attached) = self
            .get(session_id)
            .await
            .filter(|main| main.tmux_session.as_deref() == Some(name))
        {
            return Ok(attached);
        }
        self.attach_main_to_tmux(session_id, worktree, os_user, kind, name)
            .await
    }
}
