//! Eager re-attach at daemon startup: every tmux-wrapped tab whose tmux session outlived the last
//! daemon is attached again, so its session is live without anyone opening it (`#session-liveness`
//! 2/4).
//!
//! Lives in its own module rather than in `connection_service.rs`, which is over its size budget
//! (`packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`).

use super::DaemonSessionHost;

/// What one startup re-attach did, session by session and tab by tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReattachReport {
    /// The tabs attached again to their live tmux sessions.
    pub reattached: Vec<ReattachedTab>,
    /// The tmux-wrapped tabs left as they were, and why (their tmux session is gone, the record
    /// names no OS user, …). Resume brings these back.
    pub left: Vec<LeftTab>,
}

/// A tab attached again to its tmux session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReattachedTab {
    pub session_id: String,
    pub terminal_id: String,
    pub tmux_session: String,
}

/// A tmux-wrapped tab not re-attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftTab {
    pub session_id: String,
    pub terminal_id: String,
    pub reason: String,
}

impl DaemonSessionHost {
    /// Re-attach every tmux-wrapped tab, main or secondary, of every session under this daemon's
    /// sessions bases whose tmux session is alive, as the OS user its `tmux` node records, after
    /// serving that user's agent-tool socket. A session with no tab roster falls back to the main
    /// `tmux_session` its `.session.yaml` records. Nothing is relaunched: a tab whose tmux session
    /// is gone is left for Resume.
    pub async fn reattach_live_tmux_tabs(&self) -> ReattachReport {
        // TODO(session-liveness 2/4): implemented in the green step.
        todo!(
            "re-attach the live tmux-wrapped tabs under {}",
            self.tddy_data_dir.display()
        )
    }
}
