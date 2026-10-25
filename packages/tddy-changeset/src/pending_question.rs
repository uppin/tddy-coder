//! The question a session is waiting on, kept in its session directory so a reader that is not
//! watching the session live — an orchestrator's `ReadSessionActivity` — can see what it is being
//! asked (`#start-parity` 6/9).
//!
//! `.session.yaml`'s `pending_elicitation` says *that* a session is waiting; this file says *on
//! what*. Two writers keep it: a `tool` session's own process when its workflow presents a
//! clarification, and the daemon when a claude-cli session's hook reports `WaitingForInput` with the
//! agent's message. Each clears it when the session stops waiting.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file, in the session directory.
pub const PENDING_QUESTION_FILE: &str = "pending-question.json";

/// One open question.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingQuestion {
    /// `select`, `multi_select`, `text`, or `permission` (a claude-cli permission prompt).
    pub kind: String,
    #[serde(default)]
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    /// Whether a free-text answer is accepted besides the options.
    #[serde(default)]
    pub allow_other: bool,
}

/// Record `question` as the one `session_dir`'s session is waiting on, replacing any before it.
pub fn write_pending_question(session_dir: &Path, question: &PendingQuestion) -> io::Result<()> {
    // TODO(session-follow): implement
    let _ = (session_dir, question);
    Err(io::Error::other(
        "TODO(session-follow): write_pending_question",
    ))
}

/// The session is no longer waiting. Clearing a session that waits on nothing is not an error.
pub fn clear_pending_question(session_dir: &Path) -> io::Result<()> {
    // TODO(session-follow): implement
    let _ = session_dir;
    Err(io::Error::other(
        "TODO(session-follow): clear_pending_question",
    ))
}

/// The question `session_dir`'s session is waiting on, if any.
pub fn read_pending_question(session_dir: &Path) -> io::Result<Option<PendingQuestion>> {
    // TODO(session-follow): implement
    let _ = session_dir;
    Err(io::Error::other(
        "TODO(session-follow): read_pending_question",
    ))
}
