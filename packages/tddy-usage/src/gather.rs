//! Evaluating a session's usage binding, and appending the tddy subagent accounting file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::token_accounting::ConversationRecord;
use crate::tree::conversation_rows;
use crate::{UsageSection, UsageSource};

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
    section
        .sources
        .iter()
        .filter_map(|source| match source {
            UsageSource::Jsonl(jsonl) => conversation_rows(jsonl, ctx),
        })
        .flatten()
        .collect()
}

/// The session's full usage snapshot: [`account`] over `section` when the agent declares one,
/// then the tddy subagent conversations the in-jail MCP server wrote to
/// `<session_dir>/egress/accounting.json`. A missing or unreadable accounting file contributes no
/// rows, never an error.
pub fn gather_session_usage(
    section: Option<&UsageSection>,
    ctx: &UsageContext,
) -> Vec<ConversationRecord> {
    let mut records = section.map(|s| account(s, ctx)).unwrap_or_default();
    records.extend(accounting_file_rows(&ctx.session_dir));
    records
}

/// The conversations in `<session_dir>/egress/accounting.json`; none when it is missing or
/// unreadable.
fn accounting_file_rows(session_dir: &Path) -> Vec<ConversationRecord> {
    #[derive(Deserialize)]
    struct AccountingFile {
        #[serde(default)]
        conversations: Vec<ConversationRecord>,
    }
    let path = session_dir.join("egress").join("accounting.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<AccountingFile>(&text) {
        Ok(file) => file.conversations,
        Err(e) => {
            log::warn!("usage: cannot parse {}: {e}", path.display());
            Vec::new()
        }
    }
}
