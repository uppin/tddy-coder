//! The name an operator or an orchestrator gives a session.
//!
//! A session has no name of its own until someone gives it one — the web's rename, the
//! `RenameSession` RPC and exec tool, or a start's `display_name`. Until then it is called by its
//! derived label ([`crate::session_label::session_display_label`]); once named, the name is the
//! label everywhere.
//!
//! Stored as `display_name` in `.session.yaml` ([`crate::SessionMetadata`]) and listed as
//! `SessionEntry.display_name`. Every writer goes through [`normalize_display_name`], so the record
//! only ever holds a trimmed, non-empty, single-line name of at most [`DISPLAY_NAME_MAX_CHARS`]
//! characters, and an empty name means "unnamed" rather than a name of nothing.
//!
//! Feature doc: docs/ft/web/session-drawer.md § Session names.

use std::path::Path;

/// The longest name a session may be given, in characters (not bytes). Long enough for a sentence
/// naming the work, short enough to fit a drawer row and a Telegram alert's first line.
pub const DISPLAY_NAME_MAX_CHARS: usize = 80;

/// Why a requested name cannot be recorded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayNameError {
    #[error("a session name is at most {max} characters; this one has {chars}")]
    TooLong { chars: usize, max: usize },
    #[error("a session name is one line: it may not contain a control character")]
    ControlCharacter,
}

/// `raw` as a session's recorded name: trimmed, and `None` when nothing is left — an empty name
/// clears the session's name. A name longer than [`DISPLAY_NAME_MAX_CHARS`] characters once trimmed,
/// or with a control character (a newline, a tab) inside it, is refused rather than cut or cleaned:
/// the caller learns the name they asked for is not the one that would be shown.
pub fn normalize_display_name(raw: &str) -> Result<Option<String>, DisplayNameError> {
    // TODO(session-display-name): green phase.
    let _ = raw;
    todo!("normalize_display_name")
}

/// Why [`rename_session`] recorded nothing.
#[derive(Debug, thiserror::Error)]
pub enum RenameSessionError {
    #[error(transparent)]
    Invalid(#[from] DisplayNameError),
    #[error("cannot read the session's record: {0}")]
    Unreadable(crate::WorkflowError),
    #[error("cannot record the session's name: {0}")]
    Unwritable(crate::WorkflowError),
}

/// Record `raw`, normalised, as the name of the session in `session_dir`, or clear its name when
/// `raw` is empty; the name as recorded. Every other field of the record is left as it was —
/// `updated_at` included, since naming a session is not activity in it.
pub fn rename_session(session_dir: &Path, raw: &str) -> Result<Option<String>, RenameSessionError> {
    // TODO(session-display-name): green phase.
    let _ = (session_dir, raw);
    todo!("rename_session")
}
