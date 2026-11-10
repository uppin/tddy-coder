//! The conversations a session opened besides its main one — the coding subagents the daemon ran
//! for it — each with what its own agent's binding resolves against (`#usage-binding 3/5`).

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
    // TODO(live-tap): implement
    let _ = (session_dir, conversation);
    Ok(())
}

/// Every conversation registered for the session, the last registration of each id winning, in
/// first-registration order. A missing file is no conversations.
pub fn registered_conversations(session_dir: &Path) -> Vec<RegisteredConversation> {
    // TODO(live-tap): implement
    let _ = session_dir;
    Vec::new()
}

/// One row per conversation from the rows several sources of one binding produced: rows with the
/// same id merge, each counter (and the cost) taken from the source whose mapping names it, the
/// model and turns from the first source. Order is the first source's, then each new id in the
/// order it first appears.
pub fn merge_sources(per_source: Vec<Vec<ConversationRecord>>) -> Vec<ConversationRecord> {
    // TODO(live-tap): implement
    per_source.into_iter().flatten().collect()
}
