//! The end of one main-terminal re-attach attempt, announced to whoever subscribed.
//!
//! A tmux-run agent outlives its daemon, and its main terminal is re-attached lazily — on a
//! client's first stream. Whether that attach happened, and why not when it did not, was only
//! readable from the registry after the fact; this announces it where it is decided, so a caller
//! (and the logs) can wait on the outcome instead of polling for its effects.

use tokio::sync::broadcast;

use super::CliSessionManager;

/// How many unread outcomes a subscriber may fall behind by before it misses some.
pub const REATTACH_CAPACITY: usize = 256;

/// Why a re-attach left the session without a main terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReattachSkip {
    /// The session id is not a valid session directory name.
    InvalidSessionId,
    /// `.session.yaml` is missing or unreadable.
    UnreadableRecord,
    /// The record names no tmux session: the agent did not run in tmux.
    NotTmuxRun,
    /// The recorded tmux session is gone: its agent has exited.
    TmuxSessionGone,
    /// Whether the tmux session is alive could not be told.
    TmuxProbeFailed,
    /// The record names no CLI agent's session type; nothing is re-attached as a default agent.
    NoSessionType,
    /// The recorded session type could not be resolved against the installed coding agents.
    AgentUnresolved,
    /// The attach itself did not take: no live pane, or an error (logged).
    AttachFailed,
    /// A target session recorded no owner subject, so its target cannot be resolved without a
    /// caller to reach it as (`#target-reconnect` 2/3, D3): left for Resume, never guessed.
    NoOwnerSubject,
    /// The session's target could not be resolved or reached, naming why (a locked vault, a
    /// stopped workspace, a dropped connection).
    TargetUnreachable(String),
}

impl std::fmt::Display for ReattachSkip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSessionId => f.write_str("invalid session id"),
            Self::UnreadableRecord => f.write_str("unreadable session record"),
            Self::NotTmuxRun => f.write_str("the session did not run in tmux"),
            Self::TmuxSessionGone => f.write_str("its tmux session is gone"),
            Self::TmuxProbeFailed => f.write_str("its tmux session could not be probed"),
            Self::NoSessionType => f.write_str("the record names no session type"),
            Self::AgentUnresolved => {
                f.write_str("the recorded session type is not an installed agent")
            }
            Self::AttachFailed => f.write_str("the attach failed"),
            Self::NoOwnerSubject => f.write_str("the record names no owner to reach its target as"),
            Self::TargetUnreachable(reason) => {
                write!(f, "its target could not be reached: {reason}")
            }
        }
    }
}

/// How one re-attach attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReattachOutcome {
    /// The main terminal was registered by this attempt.
    Reattached,
    /// A main terminal was already registered; nothing to do.
    AlreadyAttached,
    /// No main terminal was registered, for the stated reason.
    Skipped(ReattachSkip),
}

/// One finished attempt: the session and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReattachFinished {
    pub session_id: String,
    pub outcome: ReattachOutcome,
}

pub(crate) type ReattachSender = broadcast::Sender<ReattachFinished>;

pub(crate) fn reattach_channel() -> ReattachSender {
    let (tx, _) = broadcast::channel(REATTACH_CAPACITY);
    tx
}

impl CliSessionManager {
    /// Every re-attach attempt that finishes from now on.
    pub fn reattach_finished(&self) -> broadcast::Receiver<ReattachFinished> {
        self.reattach_tx.subscribe()
    }

    /// Announce (and log) how the re-attach of `session_id` ended.
    pub fn announce_reattach(&self, session_id: &str, outcome: ReattachOutcome) {
        match outcome {
            ReattachOutcome::Reattached | ReattachOutcome::AlreadyAttached => log::info!(
                target: "tddy_cli_sessions::reattach",
                "re-attach of {session_id} finished: {outcome:?}"
            ),
            ReattachOutcome::Skipped(ref reason) => log::info!(
                target: "tddy_cli_sessions::reattach",
                "re-attach of {session_id} finished: skipped, {reason}"
            ),
        }
        // No subscriber is not an error: nobody is waiting.
        let _ = self.reattach_tx.send(ReattachFinished {
            session_id: session_id.to_string(),
            outcome,
        });
    }
}
