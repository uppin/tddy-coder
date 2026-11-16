//! The end of a session's agent, announced to whoever subscribed (`#activity-wake` 1/3).
//!
//! A main terminal ending is not always its agent ending: a tmux client can end while its agent
//! lives on in the tmux session. So the announcement is made where the manager *knows* the agent
//! is gone — a main classified [`MainExit::AgentEnded`](super::MainExit::AgentEnded), and a
//! detached agent whose tmux session the watcher found gone — and nowhere else. A tab never
//! announces.
//!
//! The daemon turns each [`AgentEnd`] into a session notification; this crate knows nothing of
//! notifications.

use tokio::sync::broadcast;

use super::CliSessionManager;

/// How many unread ends a subscriber may fall behind by before it misses some. Agent ends are rare
/// — one per session stop — so this is far past any burst a host produces; a subscriber that lags
/// anyway is told how many it missed (`RecvError::Lagged`).
pub const AGENT_END_CAPACITY: usize = 256;

/// One session's agent having ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnd {
    pub session_id: String,
    /// The OS user the agent ran as; `None` when it ran as the daemon's own user.
    pub os_user: Option<String>,
    /// How it ended, worded as the `main exited` log line words it: `exit code 3`, `exit code
    /// unknown`, `stopped by the daemon`, or a failure's message.
    pub outcome: String,
    /// The agent's exit code, when one was seen. `None` for a failure, a stop, or an agent whose
    /// end only the detach watcher saw.
    pub exit_code: Option<i32>,
}

/// The sending half every announcing site holds a clone of.
pub(crate) type AgentEndSender = broadcast::Sender<AgentEnd>;

/// A fresh channel for a new manager. The initial receiver is dropped: an end nobody subscribed to
/// yet is not kept.
pub(crate) fn agent_end_channel() -> AgentEndSender {
    let (tx, _) = broadcast::channel(AGENT_END_CAPACITY);
    tx
}

impl CliSessionManager {
    /// Every agent end from now on: one [`AgentEnd`] per session whose agent ended, announced
    /// after its exit hook ran and its terminal was deregistered.
    pub fn agent_ends(&self) -> broadcast::Receiver<AgentEnd> {
        self.agent_end_tx.subscribe()
    }
}
