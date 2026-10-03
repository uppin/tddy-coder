//! Taking a conversation's worktree back to an earlier point, for a caller that rewound the
//! conversation's transcript to a message and wants its files to follow.

use serde::{Deserialize, Serialize};

use crate::git::git;
use crate::serialise;
use crate::worktree::{ConversationWorktree, WorktreeError};

/// Where a reset goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetTarget {
    /// The conversation's base: every subagent commit is dropped.
    Base,
    /// A commit on the conversation's branch, by any unambiguous abbreviation of its hash.
    Commit(String),
}

/// What a reset did — the `worktreeReset` object on a resumed turn's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeReset {
    /// The short hash the worktree now stands at.
    pub to: String,
    /// The short hashes the reset dropped from the branch, oldest first. A list rather than a
    /// count: a caller that already took some of them (`subagent_pull`) needs to know which.
    pub dropped_commits: Vec<String>,
}

impl ConversationWorktree {
    /// Hard-reset the worktree and its branch to `target`, removing untracked files the dropped
    /// commits created (ignored files are kept — a reset must not delete a build's output). A commit
    /// that is neither the base nor one of the subagent's commits after it — a sync merge included —
    /// is refused, and nothing moves. A reset past a sync drops it, and lists only the subagent's
    /// commits as dropped.
    pub async fn reset_to(&self, target: &ResetTarget) -> Result<WorktreeReset, WorktreeError> {
        let _exclusive = serialise::exclusive(self.root()).await;
        let subagent_commits = self.subagent_commits().await?;
        let subagent_commits: Vec<&str> = subagent_commits.iter().map(String::as_str).collect();
        let (target, dropped) = self.resolve_reset(target, &subagent_commits).await?;
        let dropped_commits = self.short_hashes(&dropped).await?;
        git(self.root(), ["reset", "-q", "--hard", &target], &[], None).await?;
        git(self.root(), ["clean", "-q", "-fd"], &[], None).await?;
        let to = self.short_hash(&target).await?;
        Ok(WorktreeReset {
            to,
            dropped_commits,
        })
    }

    /// The full hash a reset to `target` lands on, and the entries of `subagent_commits` (oldest
    /// first) it drops. A commit that is not in `subagent_commits` is refused.
    async fn resolve_reset<'a>(
        &self,
        target: &ResetTarget,
        subagent_commits: &[&'a str],
    ) -> Result<(String, Vec<&'a str>), WorktreeError> {
        let commit = match target {
            ResetTarget::Base => return Ok((self.base().to_string(), subagent_commits.to_vec())),
            ResetTarget::Commit(commit) => commit,
        };
        let full = self.resolve_commit(commit).await?;
        let kept = subagent_commits
            .iter()
            .position(|hash| *hash == full)
            .ok_or_else(|| WorktreeError::Git {
                args: vec!["reset".to_string(), commit.clone()],
                stderr: format!(
                    "{commit} is not one of the subagent's commits on branch {}",
                    self.branch()
                ),
            })?;
        Ok((full, subagent_commits[kept + 1..].to_vec()))
    }

    /// The full hash `abbreviation` names, which must name a commit.
    pub(crate) async fn resolve_commit(&self, abbreviation: &str) -> Result<String, WorktreeError> {
        let found = git(
            self.root(),
            [
                "rev-parse",
                "--verify",
                &format!("{abbreviation}^{{commit}}"),
            ],
            &[],
            None,
        )
        .await?;
        Ok(found.trim().to_string())
    }

    /// The short form of `full`.
    pub(crate) async fn short_hash(&self, full: &str) -> Result<String, WorktreeError> {
        self.short_hashes(&[full])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| WorktreeError::Git {
                args: vec!["rev-list".to_string(), full.to_string()],
                stderr: format!("git listed no short hash for {full}"),
            })
    }

    /// The short form of each of `full`, in order.
    pub(crate) async fn short_hashes(&self, full: &[&str]) -> Result<Vec<String>, WorktreeError> {
        if full.is_empty() {
            return Ok(Vec::new());
        }
        let shortened = git(
            self.root(),
            ["rev-list", "--no-walk=unsorted", "--abbrev-commit"]
                .into_iter()
                .chain(full.iter().copied()),
            &[],
            None,
        )
        .await?;
        Ok(shortened.lines().map(str::to_string).collect())
    }
}
