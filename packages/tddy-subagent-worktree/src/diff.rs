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
}

impl ConversationWorktree {
    /// The diff `from..to`. `from` defaults to the base, `to` to the branch tip. Each must be the base
    /// or a commit on the branch after it, and `from` an ancestor of `to`; anything else is refused.
    /// Nothing is written.
    pub async fn diff(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<ConversationDiff, WorktreeError> {
        let range = format!("{}..{}", self.base(), self.branch());
        let on_branch = git(self.root(), ["rev-list", &range], &[], None).await?;
        let on_branch: Vec<&str> = on_branch.lines().collect();
        let from = self.resolve_bound(from, self.base(), &on_branch).await?;
        let to = self.resolve_bound(to, self.branch(), &on_branch).await?;
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
        let text = self.diff_output(&[], &from, &to).await?;
        let name_status = self.diff_output(&["--name-status"], &from, &to).await?;
        let numstat = self.diff_output(&["--numstat"], &from, &to).await?;
        let (files, lines) = count_change(&name_status, &numstat);
        let (diff, truncated) = cut_at_a_line(text, DIFF_TEXT_CAP_BYTES);
        let mut shortened = self
            .short_hashes(&[from.as_str(), to.as_str()])
            .await?
            .into_iter();
        let (from, to) = (
            shortened.next().unwrap_or_default(),
            shortened.next().unwrap_or_default(),
        );
        Ok(ConversationDiff {
            from,
            to,
            files,
            lines,
            diff,
            truncated,
        })
    }

    /// `git diff <format> from to`, with no external diff driver or text conversion run.
    async fn diff_output(
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

    /// The full hash of `bound`, or of `default` when omitted. It must be the base or one of
    /// `on_branch` (the commits after the base); anything else is refused.
    async fn resolve_bound(
        &self,
        bound: Option<&str>,
        default: &str,
        on_branch: &[&str],
    ) -> Result<String, WorktreeError> {
        let named = bound.unwrap_or(default);
        let full = self.resolve_commit(named).await?;
        if full == self.base() || on_branch.contains(&full.as_str()) {
            return Ok(full);
        }
        Err(WorktreeError::Git {
            args: vec!["diff".to_string(), named.to_string()],
            stderr: format!(
                "{named} is not the base or a commit of branch {} after it",
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
