//! The usage context a session's launch records, so whoever accounts the session later — live or
//! long after it ended — evaluates its agent's binding against the right files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::token_accounting::ConversationRecord;
use crate::{gather_session_usage, UsageContext, UsageSection};

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
/// relaunch records the context it recomputed). The file is written to a sibling and renamed, so a
/// reader never sees a half-written one.
pub fn write_usage_context(session_dir: &Path, context: &UsageContextFile) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(context)?;
    let path = session_dir.join(USAGE_CONTEXT_FILE);
    let tmp = session_dir.join(format!("{USAGE_CONTEXT_FILE}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)
}

/// The context `<session_dir>/usage-context.json` records, or `None` when the session has none
/// (it predates the file, or its launch never got that far) or it does not parse.
pub fn read_usage_context(session_dir: &Path) -> Option<UsageContextFile> {
    let path = session_dir.join(USAGE_CONTEXT_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&text) {
        Ok(context) => Some(context),
        Err(e) => {
            log::warn!("usage: cannot parse {}: {e}", path.display());
            None
        }
    }
}

/// The session's usage snapshot from its recorded context: the binding `binding_for` returns for
/// its spec, evaluated by [`crate::gather_session_usage`]. A session with no context reports only
/// its tddy subagent conversations.
pub fn gather_session_usage_for(
    session_dir: &Path,
    binding_for: &dyn Fn(&str) -> Option<UsageSection>,
) -> Vec<ConversationRecord> {
    let Some(file) = read_usage_context(session_dir) else {
        let ctx = UsageContext {
            agent_home: None,
            session_dir: session_dir.to_path_buf(),
            conversation_id: None,
            worktree: None,
            agent_label: String::new(),
            fallback_model: String::new(),
        };
        return gather_session_usage(None, &ctx);
    };
    let section = binding_for(&file.spec_id);
    let ctx = UsageContext {
        agent_home: file.agent_home,
        session_dir: session_dir.to_path_buf(),
        conversation_id: file.conversation_id,
        worktree: file.worktree,
        agent_label: file.spec_id,
        fallback_model: file.model.unwrap_or_default(),
    };
    gather_session_usage(section.as_ref(), &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> UsageContextFile {
        UsageContextFile {
            spec_id: "some-agent".into(),
            agent_home: Some(PathBuf::from("/home/u")),
            conversation_id: Some("c1".into()),
            worktree: None,
            model: Some("m".into()),
        }
    }

    #[test]
    fn a_written_context_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        write_usage_context(dir.path(), &context()).unwrap();
        assert_eq!(read_usage_context(dir.path()), Some(context()));
    }

    #[test]
    fn a_later_write_replaces_the_earlier_context() {
        let dir = tempfile::tempdir().unwrap();
        write_usage_context(dir.path(), &context()).unwrap();
        let mut later = context();
        later.agent_home = Some(PathBuf::from("/jail/home"));
        write_usage_context(dir.path(), &later).unwrap();
        assert_eq!(read_usage_context(dir.path()), Some(later));
    }

    #[test]
    fn a_missing_or_unparseable_context_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_usage_context(dir.path()), None);
        std::fs::write(dir.path().join(USAGE_CONTEXT_FILE), "{not json").unwrap();
        assert_eq!(read_usage_context(dir.path()), None);
    }
}
