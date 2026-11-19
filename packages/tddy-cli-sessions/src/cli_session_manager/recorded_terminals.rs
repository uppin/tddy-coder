//! Starting a terminal under the id a session's tab roster recorded for it, rather than a freshly
//! minted one — what Resume needs to bring a tab back as the same tab (`#session-liveness` 2/4 owns
//! the entry point; 4/4 calls it). The roster store every tab start and stop writes through is held
//! here too, so there is exactly one per daemon.

use std::path::PathBuf;
use std::sync::Arc;

use super::{pty_handle, CliSessionManager};
use crate::tab_roster_store::TabRosterStore;

/// What a terminal started under a recorded id runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedTerminal {
    /// A shell (`shell_path`) in the session's worktree, kind `"bash"`.
    Shell { shell_path: String },
    /// An agent CLI of `kind` (its session type) running `argv`.
    Agent { kind: String, argv: Vec<String> },
}

impl CliSessionManager {
    /// Start `terminal` for `session_id` under `terminal_id`. Refused for the reserved main id (main
    /// is restored by its own resume path) and for an id that is already live in this session.
    pub async fn start_terminal_under_id(
        &self,
        session_id: &str,
        terminal_id: &str,
        worktree: PathBuf,
        terminal: RecordedTerminal,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        // TODO(session-liveness 2/4): implemented in the green step.
        todo!(
            "start {terminal:?} for {session_id} under {terminal_id} in {}",
            worktree.display()
        )
    }

    /// The daemon's one tab-roster writer.
    pub fn tab_rosters(&self) -> &TabRosterStore {
        &self.tab_rosters
    }
}
