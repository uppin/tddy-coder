//! Bringing a conversation's worktree up to its caller's current files before a turn.
//!
//! A conversation worktree is cut once, from the caller's files at its first write, and afterwards
//! moves only through the subagent's tool calls. A caller that edits its own worktree between turns
//! would otherwise send the subagent back into a tree the caller no longer has. The sync merges the
//! caller's current state — `HEAD` plus staged, unstaged and untracked changes, snapshotted as the
//! conversation's start is — into the conversation's branch as a **merge commit**: first parent the
//! subagent's tip, second parent the caller snapshot, merged against the last caller state the
//! conversation took in. The subagent's own work is therefore always the branch's first-parent line
//! without merges ([`crate::ConversationWorktree::subagent_commits`]).

use serde::{Deserialize, Serialize};

use crate::change_facts::{count_change, FileCounts, LineCounts};
use crate::git::{git, git_raw};
use crate::inherit;
use crate::serialise;
use crate::worktree::{ConversationWorktree, WorktreeError};

/// How many changed paths a sync names; the rest are counted in [`WorktreeSync::more_paths`].
pub const SYNC_NOTICE_PATHS: usize = 20;

/// The subject a sync's merge commit carries.
pub const SYNC_MERGE_SUBJECT: &str = "Merge the caller's changes";

/// The subject of the commit a sync makes of uncommitted changes it finds in the conversation
/// worktree — the subagent's own work (a background job that finished between turns).
pub const OUTSIDE_A_TOOL_CALL_SUBJECT: &str = "Changes made outside a tool call";

/// `git merge-tree --write-tree` exits 0 for a clean merge.
const MERGE_TREE_CLEAN: i32 = 0;
/// `git merge-tree --write-tree` exits 1 for a merge with conflicts; any other code is a merge that
/// could not be attempted.
const MERGE_TREE_CONFLICTED: i32 = 1;

/// What a sync merged — the `worktreeSync` object on a turn's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSync {
    /// Short hash of the merge commit.
    pub commit: String,
    /// What the merge changed in the conversation worktree.
    pub files: FileCounts,
    pub lines: LineCounts,
    /// The changed paths, sorted, at most [`SYNC_NOTICE_PATHS`].
    pub paths: Vec<String>,
    /// How many changed paths `paths` leaves out.
    pub more_paths: usize,
}

/// What a sync did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The caller's files are what the conversation last took in: nothing merged.
    Unchanged,
    /// The caller's changes were merged.
    Merged(WorktreeSync),
    /// The caller's changes and the subagent's work touch the same lines in these paths; nothing
    /// was merged and nothing moved.
    Conflicted { paths: Vec<String> },
}

impl ConversationWorktree {
    /// Merge the caller's current files into the conversation's branch and worktree.
    ///
    /// Uncommitted changes already in the conversation worktree are the subagent's: a sync that goes
    /// ahead commits them first, subject [`OUTSIDE_A_TOOL_CALL_SUBJECT`]; a conflicted one leaves
    /// them uncommitted and moves nothing. The caller's worktree, index and branch are only read.
    ///
    /// The merge is 3-way against the last caller state the conversation took in, not git's own
    /// merge base: a conversation that started from the caller's uncommitted changes starts from a
    /// commit off the caller's history, and each later snapshot is a fresh one.
    pub async fn sync_with_caller(&self) -> Result<SyncOutcome, WorktreeError> {
        let _exclusive = serialise::exclusive(self.root()).await;
        // The tip itself when the worktree is clean; else its uncommitted state as a commit on top
        // of the tip that no branch points at yet.
        let ours =
            inherit::uncommitted_state_as_commit(self.root(), OUTSIDE_A_TOOL_CALL_SUBJECT).await?;
        let snapshot = inherit::base_commit(self.caller()).await?;
        let taken = self.caller_state_taken_in().await?;
        // A snapshot's hash differs from call to call; its tree says whether the files did.
        if self.tree_of(&snapshot).await? == self.tree_of(&taken).await? {
            // `reset` without `--hard`: the branch takes the commit, the files stay as they are.
            git(self.root(), ["reset", "-q", &ours], &[], None).await?;
            return Ok(SyncOutcome::Unchanged);
        }
        let tree = match self.merge_tree(&taken, &ours, &snapshot).await? {
            MergedTree::Clean(tree) => tree,
            MergedTree::Conflicted(paths) => return Ok(SyncOutcome::Conflicted { paths }),
        };
        let merge = self.record_merge(&tree, &ours, &snapshot).await?;
        let sync = self.what_the_merge_changed(&ours, &merge).await?;
        // The caller's files changed only by what the conversation already had (the caller pulled
        // the subagent's work, nothing else): the merge is recorded, so the next sync merges against
        // it, but the worktree's files are as they were and there is nothing to tell the subagent.
        if sync.paths.is_empty() {
            return Ok(SyncOutcome::Unchanged);
        }
        Ok(SyncOutcome::Merged(sync))
    }

    /// The tree `commit` records.
    async fn tree_of(&self, commit: &str) -> Result<String, WorktreeError> {
        let tree = git(
            self.root(),
            ["rev-parse", &format!("{commit}^{{tree}}")],
            &[],
            None,
        )
        .await?;
        Ok(tree.trim().to_string())
    }

    /// `git merge-tree --write-tree` of `ours` and `theirs` against `base`, writing nothing but
    /// objects; see [`parse_merge_tree`] for what its answer means.
    async fn merge_tree(
        &self,
        base: &str,
        ours: &str,
        theirs: &str,
    ) -> Result<MergedTree, WorktreeError> {
        let (args, merged) = git_raw(
            self.root(),
            [
                "merge-tree",
                "--write-tree",
                "--name-only",
                "-z",
                "--merge-base",
                base,
                ours,
                theirs,
            ],
            &[],
            None,
        )
        .await?;
        parse_merge_tree(merged.code, &merged.stdout).map_err(|unreadable| WorktreeError::Git {
            args,
            stderr: format!("{unreadable}: {}", merged.stderr.trim()),
        })
    }

    /// Commit `tree` as the merge of `ours` (first parent) and `theirs`, and move the branch and the
    /// worktree's files to it. `ours` — the tip, or the subagent's uncommitted work on top of it —
    /// thereby joins the branch's first-parent line. Returns the merge's full hash.
    async fn record_merge(
        &self,
        tree: &str,
        ours: &str,
        theirs: &str,
    ) -> Result<String, WorktreeError> {
        let merge = git(
            self.root(),
            [
                "commit-tree",
                tree,
                "-p",
                ours,
                "-p",
                theirs,
                "-m",
                SYNC_MERGE_SUBJECT,
            ],
            &[],
            None,
        )
        .await?;
        let merge = merge.trim().to_string();
        git(self.root(), ["reset", "-q", "--hard", &merge], &[], None).await?;
        Ok(merge)
    }

    /// What moving the conversation worktree from `ours` to `merge` changed, its paths sorted and
    /// capped at [`SYNC_NOTICE_PATHS`].
    async fn what_the_merge_changed(
        &self,
        ours: &str,
        merge: &str,
    ) -> Result<WorktreeSync, WorktreeError> {
        // Without rename detection every path is one entry: the list names what changed, as `paths`
        // must, and a rename counts as one removed and one created either way.
        let name_status = self
            .diff_output(&["--no-renames", "--name-status"], ours, merge)
            .await?;
        let numstat = self
            .diff_output(&["--no-renames", "--numstat"], ours, merge)
            .await?;
        let named = self
            .diff_output(&["--no-renames", "--name-only", "-z"], ours, merge)
            .await?;
        let (files, lines) = count_change(&name_status, &numstat);
        let (paths, more_paths) = capped_paths(&named);
        Ok(WorktreeSync {
            commit: self.short_hash(merge).await?,
            files,
            lines,
            paths,
            more_paths,
        })
    }
}

/// What `git merge-tree --write-tree` produced.
#[derive(Debug, PartialEq, Eq)]
enum MergedTree {
    Clean(String),
    Conflicted(Vec<String>),
}

/// Read `git merge-tree --write-tree --name-only -z`'s exit `code` and `stdout`.
///
/// The stdout is NUL-separated: field 0 is the merged tree, the fields after it are the conflicted
/// paths (sorted and deduplicated here), and an empty field ends that list (git's informational
/// messages follow it). An answer this cannot use — an exit code other than clean or conflicted, no
/// tree, or a conflict naming no path — is refused with the reason.
fn parse_merge_tree(code: Option<i32>, stdout: &[u8]) -> Result<MergedTree, &'static str> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut fields = stdout.split('\0');
    let tree = fields.next().unwrap_or_default().trim();
    if tree.is_empty() || !tree.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("git merge-tree wrote no merged tree");
    }
    match code {
        Some(MERGE_TREE_CLEAN) => Ok(MergedTree::Clean(tree.to_string())),
        Some(MERGE_TREE_CONFLICTED) => {
            let mut paths: Vec<String> = fields
                .take_while(|field| !field.is_empty())
                .map(str::to_string)
                .collect();
            if paths.is_empty() {
                return Err("git merge-tree reported a conflict naming no path");
            }
            paths.sort();
            paths.dedup();
            Ok(MergedTree::Conflicted(paths))
        }
        _ => Err("git merge-tree could not attempt the merge"),
    }
}

/// The NUL-separated `named` paths, sorted, the first [`SYNC_NOTICE_PATHS`] of them, and how many
/// more there are.
fn capped_paths(named: &str) -> (Vec<String>, usize) {
    let mut paths: Vec<String> = named
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    let more_paths = paths.len().saturating_sub(SYNC_NOTICE_PATHS);
    paths.truncate(SYNC_NOTICE_PATHS);
    (paths, more_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    fn merge_tree_answer(fields: &[&str]) -> Vec<u8> {
        fields.join("\0").into_bytes()
    }

    #[test]
    fn a_clean_merge_answers_its_tree() {
        // Given
        let stdout = merge_tree_answer(&[TREE, ""]);

        // When
        let parsed = parse_merge_tree(Some(MERGE_TREE_CLEAN), &stdout);

        // Then
        assert_eq!(parsed, Ok(MergedTree::Clean(TREE.to_string())));
    }

    #[test]
    fn a_conflicted_merge_answers_its_paths_sorted_once_each() {
        // Given — git names a path once per conflicted stage; its messages follow an empty field
        let stdout = merge_tree_answer(&[
            TREE,
            "src/lib.rs",
            "README.md",
            "src/lib.rs",
            "",
            "Auto-merging",
        ]);

        // When
        let parsed = parse_merge_tree(Some(MERGE_TREE_CONFLICTED), &stdout);

        // Then
        assert_eq!(
            parsed,
            Ok(MergedTree::Conflicted(vec![
                "README.md".to_string(),
                "src/lib.rs".to_string()
            ]))
        );
    }

    #[test]
    fn a_conflict_naming_no_path_is_refused() {
        // Given
        let stdout = merge_tree_answer(&[TREE, "", "Auto-merging"]);

        // When
        let parsed = parse_merge_tree(Some(MERGE_TREE_CONFLICTED), &stdout);

        // Then
        assert_eq!(
            parsed,
            Err("git merge-tree reported a conflict naming no path")
        );
    }

    #[test]
    fn an_answer_without_a_tree_is_refused() {
        // Given
        let stdout = merge_tree_answer(&["", ""]);

        // When
        let parsed = parse_merge_tree(Some(MERGE_TREE_CLEAN), &stdout);

        // Then
        assert_eq!(parsed, Err("git merge-tree wrote no merged tree"));
    }

    #[test]
    fn any_other_exit_code_is_refused() {
        // Given
        let stdout = merge_tree_answer(&[TREE, ""]);

        // When
        let parsed = parse_merge_tree(Some(128), &stdout);

        // Then
        assert_eq!(parsed, Err("git merge-tree could not attempt the merge"));
    }

    #[test]
    fn paths_past_the_limit_are_counted() {
        // Given
        let named: String = (0..SYNC_NOTICE_PATHS + 2)
            .rev()
            .map(|n| format!("f{n:02}\0"))
            .collect();

        // When
        let (paths, more) = capped_paths(&named);

        // Then
        assert_eq!(
            (paths.first().cloned(), paths.len(), more),
            (Some("f00".to_string()), SYNC_NOTICE_PATHS, 2)
        );
    }
}
