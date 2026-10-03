//! Bringing a conversation's worktree up to the caller's current files before a turn.
//!
//! The usual hand-off — the subagent finishes, the caller takes some of the work and edits files
//! itself, then prompts or resumes the conversation — would otherwise send the subagent back into a
//! tree the caller no longer has. Before every turn the conversation asks the host that owns its
//! worktree, through this port, to merge the caller's current files in
//! (`tddy_subagent_worktree::ConversationWorktree::sync_with_caller`). A merge is announced to the
//! model with [`sync_notice`]; a conflict refuses the turn before any model call.

use async_trait::async_trait;

pub use tddy_subagent_worktree::{WorktreeSync, SYNC_NOTICE_PATHS};

use super::SubagentError;

/// What the host answered a sync with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAnswer {
    /// The caller's changes were merged into the conversation worktree.
    Merged(WorktreeSync),
    /// Nothing to merge: no worktree yet, or the caller has not changed since the last sync.
    Nothing,
    /// The caller's changes and the subagent's unpulled work touch the same lines in these paths;
    /// nothing was merged.
    Conflicted(Vec<String>),
}

/// How a conversation asks the host that owns its worktree to take in the caller's current files.
#[async_trait]
pub trait WorktreeSyncPort: Send + Sync {
    async fn sync(&self) -> Result<SyncAnswer, SubagentError>;
}

/// The user message that tells the model its files changed under it — the paths the sync names
/// (at most [`SYNC_NOTICE_PATHS`]), how many more there were, and what the lines came to.
pub fn sync_notice(sync: &WorktreeSync) -> String {
    // TODO(caller-sync): implement
    let _ = sync;
    todo!("sync_notice")
}

/// Why a turn was refused when the caller's changes conflict: every conflicted path and the three
/// ways forward.
pub fn sync_refusal(conflicted: &[String]) -> SubagentError {
    // TODO(caller-sync): implement
    let _ = conflicted;
    todo!("sync_refusal")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_subagent_worktree::{FileCounts, LineCounts};

    fn a_sync_of(paths: &[&str], more_paths: usize) -> WorktreeSync {
        WorktreeSync {
            commit: "7d1e0aa".to_string(),
            files: FileCounts {
                created: 1,
                updated: 2,
                removed: 0,
            },
            lines: LineCounts {
                added: 41,
                removed: 7,
            },
            paths: paths.iter().map(|p| p.to_string()).collect(),
            more_paths,
        }
    }

    #[test]
    fn the_notice_names_the_files_and_the_lines() {
        assert_eq!(
            sync_notice(&a_sync_of(&["README.md", "src/lib.rs", "src/new.rs"], 0)),
            "The caller changed 3 files since your last turn (+41 −7): README.md, src/lib.rs, \
             src/new.rs. Your worktree now has their versions — re-read before relying on what you \
             read earlier."
        );
    }

    #[test]
    fn the_notice_counts_the_paths_it_does_not_name() {
        assert_eq!(
            sync_notice(&a_sync_of(&["a.rs", "b.rs"], 5)),
            "The caller changed 7 files since your last turn (+41 −7): a.rs, b.rs and 5 more. Your \
             worktree now has their versions — re-read before relying on what you read earlier."
        );
    }

    #[test]
    fn the_refusal_names_every_conflicted_path_and_the_ways_forward() {
        assert_eq!(
            sync_refusal(&["src/lib.rs".to_string(), "README.md".to_string()]).to_string(),
            "the caller's changes conflict with the subagent's unpulled work in src/lib.rs, \
             README.md — pull those commits first (subagent_pull), change the files, or carry on \
             without the caller's changes (syncWorktree: false)"
        );
    }
}
