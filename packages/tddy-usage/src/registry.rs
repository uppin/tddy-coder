//! The conversations a session opened besides its main one — the coding subagents the daemon ran
//! for it — each with what its own agent's binding resolves against (`#usage-binding 3/5`).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
