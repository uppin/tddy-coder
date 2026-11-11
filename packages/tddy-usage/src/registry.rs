//! The conversations a session opened besides its main one — the coding subagents the daemon ran
//! for it — each with what its own agent's binding resolves against (`#usage-binding 3/5`).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::token_accounting::ConversationRecord;

/// The registry file in the session directory: one JSON line per registration, appended.
pub const CONVERSATION_REGISTRY_FILE: &str = "usage-conversations.jsonl";

/// One conversation a session opened: which agent ran it, where its files are, and the
/// conversation it is nested under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredConversation {
    pub spec_id: String,
    pub conversation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<PathBuf>,
    /// The conversation this one is nested under — the session's main conversation for a subagent
    /// the session opened.
    pub parent_id: String,
    /// The row's `agent` — the subagent's name.
    pub agent_label: String,
}

/// Append `conversation` to `<session_dir>/usage-conversations.jsonl`. Appends are atomic per line,
/// so concurrent opens need no lock; registering a conversation again (a resume) supersedes it.
pub fn register_conversation(
    session_dir: &Path,
    conversation: &RegisteredConversation,
) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(conversation)?;
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(session_dir.join(CONVERSATION_REGISTRY_FILE))?;
    // One `write_all` of the whole line: with `O_APPEND` the kernel keeps concurrent lines whole.
    file.write_all(&line)
}

/// Every conversation registered for the session, the last registration of each id winning, in
/// first-registration order. A missing file is no conversations.
pub fn registered_conversations(session_dir: &Path) -> Vec<RegisteredConversation> {
    let path = session_dir.join(CONVERSATION_REGISTRY_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut registered: Vec<RegisteredConversation> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<RegisteredConversation>(line) {
            Ok(conversation) => {
                match registered
                    .iter_mut()
                    .find(|c| c.conversation_id == conversation.conversation_id)
                {
                    Some(existing) => *existing = conversation,
                    None => registered.push(conversation),
                }
            }
            Err(e) => log::warn!("usage: skipping a line of {}: {e}", path.display()),
        }
    }
    registered
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
