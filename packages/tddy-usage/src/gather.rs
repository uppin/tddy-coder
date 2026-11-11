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
    merge_sources(
        section
            .sources
            .iter()
            .filter_map(|source| match source {
                UsageSource::Jsonl(jsonl) => conversation_rows(jsonl, ctx),
            })
            .collect(),
    )
}

/// One row per conversation from the rows several sources of one binding produced: rows with the
/// same id merge, each counter (and the cost) taken from the source whose mapping names it, the
/// model and turns from the first source. Order is the first source's, then each new id in the
/// order it first appears.
///
/// A counter is mapped by at most one source (the spec loader refuses overlaps), so a source that
/// does not map it reads zero and the plain sum is the mapping source's figure. The model is the
/// first source's, or the first non-empty one when that source named none.
pub fn merge_sources(per_source: Vec<Vec<ConversationRecord>>) -> Vec<ConversationRecord> {
    let mut merged: Vec<ConversationRecord> = Vec::new();
    for row in per_source.into_iter().flatten() {
        let Some(into) = merged.iter_mut().find(|r| r.id == row.id) else {
            merged.push(row);
            continue;
        };
        into.input_tokens += row.input_tokens;
        into.output_tokens += row.output_tokens;
        into.total_tokens += row.total_tokens;
        into.cache_read_tokens += row.cache_read_tokens;
        into.cache_write_tokens += row.cache_write_tokens;
        into.reasoning_tokens += row.reasoning_tokens;
        into.cost_usd = into.cost_usd.or(row.cost_usd);
        if into.model.is_empty() {
            into.model = row.model;
        }
    }
    merged
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
