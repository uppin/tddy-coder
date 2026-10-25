//! A `tool` session records the clarification its workflow is waiting on in its session directory,
//! so an orchestrator following it with `ReadSessionActivity` sees the question, not only that one
//! is open (`#start-parity` 6/9).
//!
//! The presenter's mode is the source: a select, multi-select or free-text mode is a pending
//! question, and any other mode means the session is not waiting.

use std::io;
use std::path::Path;

use tddy_core::pending_question::PendingQuestion;
use tddy_core::AppMode;

/// The question `mode` is waiting on, or `None` when it waits on nothing.
pub fn pending_question_for_mode(mode: &AppMode) -> Option<PendingQuestion> {
    // TODO(session-follow): implement
    let _ = mode;
    None
}

/// Record (or clear) `session_dir`'s pending question for `mode`.
pub fn record_pending_question(session_dir: &Path, mode: &AppMode) -> io::Result<()> {
    // TODO(session-follow): implement, and call it where the presenter's mode changes
    let _ = (session_dir, mode);
    Ok(())
}
