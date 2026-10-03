//! Reading what a conversation changed between two of its commits.

use serde::{Deserialize, Serialize};

use crate::change_facts::{count_change, FileCounts, LineCounts};
use crate::git::{git, git_raw};
use crate::worktree::{ConversationWorktree, WorktreeError};

/// The most diff text one answer carries. Past it the text is cut at the last whole line and
/// [`ConversationDiff::truncated`] is set; the counts still describe the whole range.
pub const DIFF_TEXT_CAP_BYTES: usize = 64 * 1024;

/// The changes a conversation made after `from`, up to and including `to` — git's `from..to`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationDiff {
    /// Short hash of the exclusive lower bound.
    pub from: String,
    /// Short hash of the inclusive upper bound.
    pub to: String,
    pub files: FileCounts,
    pub lines: LineCounts,
    /// Unified diff text (a binary file shows as git's `Binary files … differ`).
    pub diff: String,
    pub truncated: bool,
    /// The range spans a sync — a merge of the caller's changes — so `diff` and the counts include
    /// what the caller changed, not only what the subagent did.
    #[serde(rename = "includesCallerChanges")]
    pub includes_caller_changes: bool,
}

impl ConversationWorktree {
    /// The diff `from..to`. `from` defaults to the base, `to` to the branch tip — which may be a sync
    /// merge, flagged by [`ConversationDiff::includes_caller_changes`]. A bound that is named must be
    /// the base or one of the subagent's commits after it (a sync merge is not), and `from` an
    /// ancestor of `to`; anything else is refused.
    /// Nothing is written.
    pub async fn diff(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<ConversationDiff, WorktreeError> {
        let subagent_commits = self.subagent_commits().await?;
        let subagent_commits: Vec<&str> = subagent_commits.iter().map(String::as_str).collect();
        let from = self
            .resolve_bound(from.unwrap_or(self.base()), &subagent_commits)
            .await?;
        let to = match to {
            Some(named) => self.resolve_bound(named, &subagent_commits).await?,
            None => self.resolve_commit(self.branch()).await?,
        };
        let (shown, ancestry) = git_raw(
            self.root(),
            ["merge-base", "--is-ancestor", &from, &to],
            &[],
            None,
        )
        .await?;
        if !ancestry.success {
            return Err(WorktreeError::Git {
                args: shown,
                stderr: format!("{from} is not an ancestor of {to}"),
            });
        }
        let includes_caller_changes = self.merges_between(&from, &to).await?;
        let (text, files, lines) = self.range_changes(&from, &to).await?;
        let (diff, truncated) = cut_at_a_line(text, DIFF_TEXT_CAP_BYTES);
        let (from, to) = self.short_pair(&from, &to).await?;
        Ok(ConversationDiff {
            from,
            to,
            files,
            lines,
            diff,
            truncated,
            includes_caller_changes,
        })
    }

    /// The range's full diff text and its whole-range counts.
    async fn range_changes(
        &self,
        from: &str,
        to: &str,
    ) -> Result<(String, FileCounts, LineCounts), WorktreeError> {
        let text = self.diff_output(&[], from, to).await?;
        let name_status = self.diff_output(&["--name-status"], from, to).await?;
        let numstat = self.diff_output(&["--numstat"], from, to).await?;
        let (files, lines) = count_change(&name_status, &numstat);
        Ok((text, files, lines))
    }

    /// The short hashes of `from` and `to`.
    async fn short_pair(&self, from: &str, to: &str) -> Result<(String, String), WorktreeError> {
        let mut shortened = self.short_hashes(&[from, to]).await?.into_iter();
        Ok((
            shortened.next().unwrap_or_default(),
            shortened.next().unwrap_or_default(),
        ))
    }

    /// `git diff <format> from to`, with no external diff driver or text conversion run.
    pub(crate) async fn diff_output(
        &self,
        format: &[&str],
        from: &str,
        to: &str,
    ) -> Result<String, WorktreeError> {
        let args = ["diff", "--no-ext-diff", "--no-textconv"]
            .into_iter()
            .chain(format.iter().copied())
            .chain([from, to]);
        git(self.root(), args, &[], None).await
    }

    /// The full hash of the bound `named`, which must be the base or one of `subagent_commits`;
    /// anything else is refused.
    async fn resolve_bound(
        &self,
        named: &str,
        subagent_commits: &[&str],
    ) -> Result<String, WorktreeError> {
        let full = self.resolve_commit(named).await?;
        if full == self.base() || subagent_commits.contains(&full.as_str()) {
            return Ok(full);
        }
        Err(WorktreeError::Git {
            args: vec!["diff".to_string(), named.to_string()],
            stderr: format!(
                "{named} is not the base or one of the subagent's commits on branch {}",
                self.branch()
            ),
        })
    }
}

/// `text` unchanged when it fits in `cap` bytes; otherwise its longest prefix of whole lines within
/// `cap`, and `true`.
fn cut_at_a_line(mut text: String, cap: usize) -> (String, bool) {
    if text.len() <= cap {
        return (text, false);
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let kept = text[..end].rfind('\n').map_or(0, |newline| newline + 1);
    text.truncate(kept);
    (text, true)
}
