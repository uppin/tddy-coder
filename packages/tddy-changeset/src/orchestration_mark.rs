//! The mark an orchestrator puts on one of its sessions — its ledger of that session, kept in the
//! session's own record so any orchestrator, and the web, reads the same state (`#orchestrators`
//! 5/11).
//!
//! Stored as `orchestration_mark` in `.session.yaml` ([`crate::SessionMetadata`]) and listed as
//! `SessionEntry.orchestration_mark`. `SetSessionMark` writes it; a `none` mark removes it. A
//! [`OrchestrationMarkKind::Paused`] mark also stops the session, and any resume of the session
//! clears it ([`clear_a_paused_mark`]); `blocked` and `done` only mark.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md.

use serde::{Deserialize, Serialize};

/// What an orchestrator marked a session as. The absence of a mark is "none".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrchestrationMarkKind {
    /// Stopped on purpose by its orchestrator; any resume clears it.
    Paused,
    /// Waiting on something outside the session; it keeps running.
    Blocked,
    /// Its work is finished; it keeps running until it is stopped.
    Done,
}

impl OrchestrationMarkKind {
    /// The word a tool, the record and the listing spell it with: `paused`, `blocked` or `done`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}

/// One session's mark, as `.session.yaml` records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestrationMark {
    pub mark: OrchestrationMarkKind,
    /// Why, as the orchestrator put it; absent when it gave none. Shown on hover in the drawer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The session that set the mark.
    pub set_by_session_id: String,
    /// When the mark was set, in milliseconds since the Unix epoch.
    pub at_unix_ms: u64,
}

/// Remove `meta`'s mark when it is [`OrchestrationMarkKind::Paused`], as every resume does: a
/// resumed session is no longer paused. A `blocked` or `done` mark, and no mark, are left alone.
/// Whether a mark was removed.
pub fn clear_a_paused_mark(meta: &mut crate::SessionMetadata) -> bool {
    // TODO(session-lifecycle-tools): implement — take a `paused` mark off `meta`.
    let _ = meta;
    false
}
