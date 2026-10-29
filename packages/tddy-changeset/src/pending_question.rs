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
///
/// The flat fields (`kind`, `header`, `question`, `options`, `allow_other`) are what `#start-parity`
/// 6/9 wrote and still read. A claude-cli session's `PermissionRequest` or `AskUserQuestion` hook
/// also fills the structured ones below them (`#home-inbox` 3/8); every one is `#[serde(default)]`,
/// so a record written before they existed reads back unchanged.
///
/// For a structured record the flat fields mirror the **first** question: `kind` is `select` or
/// `multi_select` for an `AskUserQuestion` and `permission` for a permission prompt, and
/// `allow_other` is true for an `AskUserQuestion`.
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
    /// The id this question is answered by, minted when the hook reported it. Empty for a record
    /// that carries no structure (a `tool` session's, or a bare `Notification` message).
    #[serde(default)]
    pub request_id: String,
    /// The tool the agent asks to use — `Bash` for a permission, `AskUserQuestion` for a question.
    #[serde(default)]
    pub tool_name: String,
    /// The tool's input exactly as the hook sent it, as a JSON string.
    #[serde(default)]
    pub tool_input_json: String,
    /// Every question an `AskUserQuestion` asked; empty for a permission.
    #[serde(default)]
    pub questions: Vec<PendingQuestionItem>,
    /// When the daemon recorded the question, in Unix milliseconds. `0` when it was not stamped.
    #[serde(default)]
    pub at_unix_ms: u64,
}

/// One question of an `AskUserQuestion` call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingQuestionItem {
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<PendingQuestionOption>,
    /// Whether more than one option may be chosen.
    #[serde(default)]
    pub multi_select: bool,
}

/// One choice offered by a [`PendingQuestionItem`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingQuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

impl PendingQuestion {
    /// One line saying what is asked: the input summary of a permission (a `Bash` command, a file
    /// path), or the first question's text. Empty when the record says neither.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(first) = self.questions.first() {
            return first.question.clone();
        }
        if self.tool_name.is_empty() {
            return String::new();
        }
        let input = serde_json::from_str::<serde_json::Value>(&self.tool_input_json).ok();
        // The field that names what a tool acts on. Any other tool, or input without that field, is
        // summarised by its whole input.
        let named_field = match self.tool_name.as_str() {
            "Bash" => Some("command"),
            "Edit" | "Write" | "Read" => Some("file_path"),
            _ => None,
        };
        let summary = match named_field.and_then(|field| input.as_ref()?.get(field)?.as_str()) {
            Some(text) => text.to_string(),
            None => match &input {
                Some(value) => value.to_string(),
                None => self.tool_input_json.clone(),
            },
        };
        summary.replace(['\r', '\n'], " ").trim().to_string()
    }
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
