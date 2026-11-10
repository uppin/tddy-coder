//! The usage context a session's launch records, so whoever accounts the session later — live or
//! long after it ended — evaluates its agent's binding against the right files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::token_accounting::ConversationRecord;
use crate::UsageSection;

/// The file a launch writes next to the session's `.session.yaml`.
pub const USAGE_CONTEXT_FILE: &str = "usage-context.json";

/// What a session's launch knew about where its agent keeps its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContextFile {
    /// The agent spec the session runs — whose `usage:` binding applies.
    pub spec_id: String,
    /// The agent's HOME as a host path: the jail's home for a jailed session, the OS user's home
    /// for a host one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_home: Option<PathBuf>,
    /// The session's main conversation id, when its agent has one the daemon knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// The session's working tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<PathBuf>,
    /// The model a main row reports when the agent's files name none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Write `context` to `<session_dir>/usage-context.json`, replacing any earlier one (a resume or
/// relaunch records the context it recomputed).
pub fn write_usage_context(session_dir: &Path, context: &UsageContextFile) -> std::io::Result<()> {
    // TODO(daemon-stream): implement
    let _ = (session_dir, context);
    Ok(())
}

/// The context `<session_dir>/usage-context.json` records, or `None` when the session has none
/// (it predates the file, or its launch never got that far) or it does not parse.
pub fn read_usage_context(session_dir: &Path) -> Option<UsageContextFile> {
    // TODO(daemon-stream): implement
    let _ = session_dir;
    None
}

/// The session's usage snapshot from its recorded context: the binding `binding_for` returns for
/// its spec, evaluated by [`crate::gather_session_usage`]. A session with no context reports only
/// its tddy subagent conversations.
pub fn gather_session_usage_for(
    session_dir: &Path,
    binding_for: &dyn Fn(&str) -> Option<UsageSection>,
) -> Vec<ConversationRecord> {
    // TODO(daemon-stream): implement
    let _ = (session_dir, binding_for);
    Vec::new()
}
