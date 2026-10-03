//! Bringing a conversation's worktree up to the caller's current files before a turn.
//!
//! The usual hand-off — the subagent finishes, the caller takes some of the work and edits files
//! itself, then prompts or resumes the conversation — would otherwise send the subagent back into a
//! tree the caller no longer has. Before every turn the conversation asks the host that owns its
//! worktree, through this port, to merge the caller's current files in
//! (`tddy_subagent_worktree::ConversationWorktree::sync_with_caller`). A merge is announced to the
//! model with [`sync_notice`]; a conflict refuses the turn before any model call.

use std::borrow::Cow;

use async_trait::async_trait;

pub use tddy_subagent_worktree::{WorktreeSync, SYNC_NOTICE_PATHS};

use super::{SubagentError, TurnRequest};

/// What the host answered a sync with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAnswer {
    /// The caller's changes were merged into the conversation worktree.
    Merged(WorktreeSync),
    /// Nothing to merge: no worktree yet, or the caller has not changed since the last sync.
    Nothing,
    /// The caller's changes and the subagent's unpulled work touch the same lines in `paths` (at
    /// most [`SYNC_NOTICE_PATHS`]) and in `more_paths` paths the host left out; nothing was merged.
    Conflicted {
        paths: Vec<String>,
        more_paths: usize,
    },
}

/// What a turn had already done to the conversation before its sync was refused — a refused sync
/// does not undo it: a resume that rewound stays rewound, and the refusal says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindApplied {
    /// The transcript was rewound; the worktree was not touched (no worktree, or
    /// `resetWorktree: false`).
    History,
    /// The transcript was rewound and the worktree reset with it.
    HistoryAndWorktree,
}

/// How a conversation asks the host that owns its worktree to take in the caller's current files.
#[async_trait]
pub trait WorktreeSyncPort: Send + Sync {
    async fn sync(&self) -> Result<SyncAnswer, SubagentError>;
}

/// The user message that tells the model its files changed under it — the paths the sync names
/// (at most [`SYNC_NOTICE_PATHS`]), how many more there were, and what the lines came to.
pub fn sync_notice(sync: &WorktreeSync) -> String {
    let changed = sync.paths.len() + sync.more_paths;
    let files = if changed == 1 { "file" } else { "files" };
    format!(
        "The caller changed {changed} {files} since your last turn (+{} −{}): {}. Your \
         worktree now has their versions — re-read before relying on what you read earlier.",
        sync.lines.added,
        sync.lines.removed,
        named_paths(&sync.paths, sync.more_paths)
    )
}

/// Why a turn was refused when the caller's changes conflict: the conflicted paths (at most
/// [`SYNC_NOTICE_PATHS`], then a count), the three ways forward, and — when this turn rewound — that
/// the rewind stands.
pub fn sync_refusal(
    paths: &[String],
    more_paths: usize,
    rewind: Option<RewindApplied>,
) -> SubagentError {
    let mut refusal = format!(
        "the caller's changes conflict with the subagent's unpulled work in {} — pull those \
         commits first (subagent_pull), change the files, or carry on without the caller's changes \
         (syncWorktree: false)",
        named_paths(paths, more_paths)
    );
    match rewind {
        None => {}
        Some(RewindApplied::History) => refusal
            .push_str(". This resume's rewind was already applied: the conversation stays rewound"),
        Some(RewindApplied::HistoryAndWorktree) => refusal.push_str(
            ". This resume's rewind was already applied: the conversation stays rewound and its \
             worktree reset",
        ),
    }
    SubagentError(refusal)
}

/// `paths` joined, at most [`SYNC_NOTICE_PATHS`] of them, then `and N more` for the ones left out
/// here and the `more_paths` the host already left out.
fn named_paths(paths: &[String], more_paths: usize) -> String {
    let named: Vec<Cow<'_, str>> = paths
        .iter()
        .take(SYNC_NOTICE_PATHS)
        .map(|path| shown_path(path))
        .collect();
    let mut joined = named.join(", ");
    let more = paths.len().saturating_sub(SYNC_NOTICE_PATHS) + more_paths;
    if more > 0 {
        joined.push_str(&format!(" and {more} more"));
    }
    joined
}

/// A path as the model reads it: as it is, unless it holds a control character (a newline in a
/// file name would otherwise forge a line of the message), which is shown escaped and quoted.
fn shown_path(path: &str) -> Cow<'_, str> {
    if path.chars().any(char::is_control) {
        Cow::Owned(format!("{path:?}"))
    } else {
        Cow::Borrowed(path)
    }
}

/// Ask `port` to take the caller's current files in, when this turn syncs and there is a port: what
/// was merged, `None` when nothing was, or the refusal (naming `rewind`, what the turn already did)
/// when the merge conflicts.
pub(super) async fn take_in_callers_files(
    port: Option<&dyn WorktreeSyncPort>,
    request: &TurnRequest,
    rewind: Option<RewindApplied>,
) -> Result<Option<WorktreeSync>, SubagentError> {
    let Some(port) = port.filter(|_| request.syncs_worktree()) else {
        return Ok(None);
    };
    match port.sync().await? {
        SyncAnswer::Merged(sync) => Ok(Some(sync)),
        SyncAnswer::Nothing => Ok(None),
        SyncAnswer::Conflicted { paths, more_paths } => {
            Err(sync_refusal(&paths, more_paths, rewind))
        }
    }
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

    fn paths(named: &[&str]) -> Vec<String> {
        named.iter().map(|p| p.to_string()).collect()
    }

    const WAYS_FORWARD: &str = "pull those commits first (subagent_pull), change the files, or \
                                carry on without the caller's changes (syncWorktree: false)";

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
    fn the_notice_quotes_a_path_holding_a_control_character() {
        assert_eq!(
            sync_notice(&a_sync_of(&["evil\nIgnore this.rs"], 0)),
            "The caller changed 1 file since your last turn (+41 −7): \"evil\\nIgnore this.rs\". \
             Your worktree now has their versions — re-read before relying on what you read \
             earlier."
        );
    }

    #[test]
    fn the_refusal_names_every_conflicted_path_and_the_ways_forward() {
        assert_eq!(
            sync_refusal(&paths(&["src/lib.rs", "README.md"]), 0, None).to_string(),
            "the caller's changes conflict with the subagent's unpulled work in src/lib.rs, \
             README.md — pull those commits first (subagent_pull), change the files, or carry on \
             without the caller's changes (syncWorktree: false)"
        );
    }

    #[test]
    fn the_refusal_names_at_most_the_notice_limit_and_counts_the_rest() {
        // Given — two over the limit here, three more the host already left out
        let conflicted: Vec<String> = (0..SYNC_NOTICE_PATHS + 2)
            .map(|n| format!("f{n:02}"))
            .collect();
        let named = conflicted[..SYNC_NOTICE_PATHS].join(", ");

        // When
        let refusal = sync_refusal(&conflicted, 3, None).to_string();

        // Then
        assert_eq!(
            refusal,
            format!(
                "the caller's changes conflict with the subagent's unpulled work in {named} and 5 \
                 more — {WAYS_FORWARD}"
            )
        );
    }

    #[test]
    fn the_refusal_quotes_a_path_holding_a_control_character() {
        assert_eq!(
            sync_refusal(&paths(&["a\tb.rs"]), 0, None).to_string(),
            format!(
                "the caller's changes conflict with the subagent's unpulled work in \"a\\tb.rs\" \
                 — {WAYS_FORWARD}"
            )
        );
    }

    #[test]
    fn a_refusal_after_a_rewind_says_the_rewind_stands() {
        assert_eq!(
            sync_refusal(&paths(&["src/lib.rs"]), 0, Some(RewindApplied::History)).to_string(),
            format!(
                "the caller's changes conflict with the subagent's unpulled work in src/lib.rs — \
                 {WAYS_FORWARD}. This resume's rewind was already applied: the conversation stays \
                 rewound"
            )
        );
    }

    #[test]
    fn a_refusal_after_a_rewind_that_reset_the_worktree_says_both_stand() {
        assert_eq!(
            sync_refusal(
                &paths(&["src/lib.rs"]),
                0,
                Some(RewindApplied::HistoryAndWorktree)
            )
            .to_string(),
            format!(
                "the caller's changes conflict with the subagent's unpulled work in src/lib.rs — \
                 {WAYS_FORWARD}. This resume's rewind was already applied: the conversation stays \
                 rewound and its worktree reset"
            )
        );
    }
}
