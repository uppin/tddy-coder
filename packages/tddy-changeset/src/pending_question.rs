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
///
/// Written to a sibling temp file and renamed over the real one, so a concurrent reader sees the
/// whole old question or the whole new one, never a torn file.
pub fn write_pending_question(session_dir: &Path, question: &PendingQuestion) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(question).map_err(io::Error::other)?;
    let temp = session_dir.join(format!("{PENDING_QUESTION_FILE}.tmp"));
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, session_dir.join(PENDING_QUESTION_FILE))
}

/// The session is no longer waiting. Clearing a session that waits on nothing is not an error.
pub fn clear_pending_question(session_dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(session_dir.join(PENDING_QUESTION_FILE)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The question `session_dir`'s session is waiting on, if any.
pub fn read_pending_question(session_dir: &Path) -> io::Result<Option<PendingQuestion>> {
    match std::fs::read(session_dir.join(PENDING_QUESTION_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
