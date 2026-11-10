//! Evaluating a session's usage binding, and appending the tddy subagent accounting file.

use std::path::PathBuf;

use crate::token_accounting::ConversationRecord;
use crate::UsageSection;

/// What a binding's templates resolve against for one session, and what labels its main row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageContext {
    /// `{agent_home}` — the agent's HOME as a host path. `None` when the session has none.
    pub agent_home: Option<PathBuf>,
    /// `{session_dir}` — the session's directory; also where `egress/accounting.json` lives.
    pub session_dir: PathBuf,
    /// `{conversation_id}` — the main conversation's id.
    pub conversation_id: Option<String>,
    /// `{worktree}` — the session's working tree.
    pub worktree: Option<PathBuf>,
    /// The `agent` of the main conversation's row — the spec's id, passed as data.
    pub agent_label: String,
    /// The model a main row reports when its files name none.
    pub fallback_model: String,
}

/// Every conversation `section` yields for the session `ctx` describes: the main conversation
/// first (a zero row carrying `ctx.fallback_model` when it has no files yet), each followed by its
/// children, recursively, every child's `parent_id` naming its parent.
pub fn account(section: &UsageSection, ctx: &UsageContext) -> Vec<ConversationRecord> {
    // TODO(claude-transcript): implement
    let _ = (section, ctx);
    Vec::new()
}

/// The session's full usage snapshot: [`account`] over `section` when the agent declares one,
/// then the tddy subagent conversations the in-jail MCP server wrote to
/// `<session_dir>/egress/accounting.json`. A missing or unreadable accounting file contributes no
/// rows, never an error.
pub fn gather_session_usage(
    section: Option<&UsageSection>,
    ctx: &UsageContext,
) -> Vec<ConversationRecord> {
    // TODO(claude-transcript): implement
    let _ = (section, ctx);
    Vec::new()
}
