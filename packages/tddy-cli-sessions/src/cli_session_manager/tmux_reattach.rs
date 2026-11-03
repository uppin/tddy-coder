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
    // TODO(tmux-resume): implement
    let _ = (tmux_session, tmux_session_alive);
    unimplemented!("TODO(tmux-resume): classify_main_exit")
}

impl CliSessionManager {
    /// Re-attach `session_id`'s main terminal to the live tmux session `name`: what
    /// [`attach_main_to_tmux`](Self::attach_main_to_tmux) does, plus re-registering the main
    /// recording and the terminal exposure the previous client's exit left behind. Returns the
    /// new main handle. The agent is not touched.
    pub async fn reattach_main(
        &self,
        session_id: &str,
        os_user: &str,
        name: &str,
        worktree: PathBuf,
        kind: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        // TODO(tmux-resume): implement
        let _ = (self, session_id, os_user, name, worktree, kind);
        unimplemented!("TODO(tmux-resume): reattach_main")
    }
}
