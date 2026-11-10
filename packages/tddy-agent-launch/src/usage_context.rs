//! Recording the usage context of a launched session: the facts the daemon later evaluates the
//! agent's `usage:` binding against, written next to the session's `.session.yaml`
//! (`tddy_usage::write_usage_context`). Every launch, resume and relaunch of a coding agent calls
//! it once the agent's home is known; nothing here names an agent.

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_usage::{write_usage_context, UsageContextFile};

/// What a launch knows about a session's main conversation, beyond where its agent lives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageSubject {
    /// The main conversation's id, when the agent's conversation has one the daemon knows.
    pub conversation_id: Option<String>,
    /// The model the session runs, when known; empty is the same as unknown.
    pub model: Option<String>,
}

impl UsageSubject {
    /// A subject of `conversation_id` and `model`, an empty model being none.
    pub fn new(conversation_id: Option<&str>, model: &str) -> Self {
        Self {
            conversation_id: conversation_id.map(str::to_string),
            model: Some(model.trim().to_string()).filter(|model| !model.is_empty()),
        }
    }
}

/// Write the usage context of the session in `session_dir`, running agent `spec_id` with its files
/// under `agent_home` (a host path) in `worktree`, replacing any earlier one.
pub(crate) fn record_usage_context(
    session_dir: &Path,
    spec_id: &str,
    agent_home: Option<PathBuf>,
    subject: &UsageSubject,
    worktree: &Path,
) -> Result<(), Status> {
    write_usage_context(
        session_dir,
        &UsageContextFile {
            spec_id: spec_id.to_string(),
            agent_home,
            conversation_id: subject.conversation_id.clone(),
            worktree: Some(worktree.to_path_buf()),
            model: subject.model.clone(),
        },
    )
    .map_err(|e| Status::internal(format!("failed to write the session's usage context: {e}")))
}
