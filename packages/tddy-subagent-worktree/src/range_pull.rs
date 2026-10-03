//! Handing a chosen range of a conversation's commits to the caller, commit by commit, skipping the
//! ones the caller already took.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::change_facts::{count_change, FileCounts, LineCounts};
use crate::git::git;
use crate::worktree::{ConversationWorktree, WorktreeError};

/// Which commits a pull applies — both bounds **inclusive**, by short hash. `from` omitted: the
/// earliest commit not yet pulled; `to` omitted: the branch tip. The base is not a pullable commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullRange {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// What a range pull applied to the caller's worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangePullOutcome {
    /// Short hashes applied, oldest first.
    pub commits: Vec<String>,
    /// Short hashes inside the range that were already pulled and so were not applied again.
    pub skipped: Vec<String>,
    pub files: FileCounts,
    pub lines: LineCounts,
    /// Paths written with conflict markers.
    pub conflicts: Vec<String>,
}

impl ConversationWorktree {
    /// Apply each commit of `range` not in `already_pulled`, oldest first, each as its own 3-way
    /// apply into the caller's worktree as uncommitted changes. The branch and the conversation's
    /// worktree are not touched. A bound that is not one of the subagent's commits, or a `from`
    /// after `to`, is refused; a range with nothing left to pull applies nothing and is not an
    /// error. Only the subagent's commits ([`Self::subagent_commits`]) are pullable: a sync merge is
    /// the caller's own work.
    ///
    /// `files` and `lines` are the sum of what each applied commit changed.
    pub async fn pull_range(
        &self,
        range: &PullRange,
        already_pulled: &BTreeSet<String>,
    ) -> Result<RangePullOutcome, WorktreeError> {
        let subagent_commits = self.subagent_commits().await?;
        let is_pulled = |full: &str| {
            already_pulled
                .iter()
                .any(|pulled| !pulled.is_empty() && full.starts_with(pulled.as_str()))
        };
        let lower = match range.from.as_deref() {
            Some(named) => self.position_on(named, &subagent_commits).await?,
            None => match subagent_commits.iter().position(|full| !is_pulled(full)) {
                Some(first) => first,
                None => return Ok(RangePullOutcome::default()),
            },
        };
        let upper = match range.to.as_deref() {
            Some(named) => self.position_on(named, &subagent_commits).await?,
            None => match subagent_commits.len().checked_sub(1) {
                Some(tip) => tip,
                None => return Ok(RangePullOutcome::default()),
            },
        };
        if lower > upper {
            return Err(WorktreeError::Git {
                args: vec!["pull_range".to_string()],
                stderr: format!(
                    "from {} is after to {}",
                    subagent_commits[lower], subagent_commits[upper]
                ),
            });
        }
        let mut applied = Vec::new();
        let mut skipped = Vec::new();
        let mut outcome = RangePullOutcome::default();
        for full in &subagent_commits[lower..=upper] {
            if is_pulled(full) {
                skipped.push(full.clone());
                continue;
            }
            self.pull_commit(full, &mut outcome).await?;
            applied.push(full.clone());
        }
        let applied_refs: Vec<&str> = applied.iter().map(String::as_str).collect();
        let skipped_refs: Vec<&str> = skipped.iter().map(String::as_str).collect();
        outcome.commits = self.short_hashes(&applied_refs).await?;
        outcome.skipped = self.short_hashes(&skipped_refs).await?;
        Ok(outcome)
    }

    /// Where `named` sits in `subagent_commits`; a name that is not one of them is refused.
    async fn position_on(
        &self,
        named: &str,
        subagent_commits: &[String],
    ) -> Result<usize, WorktreeError> {
        let full = self.resolve_commit(named).await?;
        subagent_commits
            .iter()
            .position(|commit| *commit == full)
            .ok_or_else(|| WorktreeError::Git {
                args: vec!["pull_range".to_string(), named.to_string()],
                stderr: format!(
                    "{named} is not one of the subagent's commits on branch {}",
                    self.branch()
                ),
            })
    }

    /// Apply the one commit `full` to the caller's worktree, 3-way, adding what it changed to `outcome`.
    /// `full` is a subagent commit, never a merge, so `full^` is the one parent it was made on.
    async fn pull_commit(
        &self,
        full: &str,
        outcome: &mut RangePullOutcome,
    ) -> Result<(), WorktreeError> {
        let parent = format!("{full}^");
        let caller = self.caller();
        let name_status = git(caller, ["diff", "--name-status", &parent, full], &[], None).await?;
        if name_status.trim().is_empty() {
            return Ok(());
        }
        let numstat = git(caller, ["diff", "--numstat", &parent, full], &[], None).await?;
        let (files, lines) = count_change(&name_status, &numstat);
        let patch = git(caller, ["diff", "--binary", &parent, full], &[], None).await?;
        let touched = git(
            caller,
            ["diff", "--name-only", "--no-renames", "-z", &parent, full],
            &[],
            None,
        )
        .await?;
        let conflicts = self
            .apply_3way(patch.as_bytes(), touched.as_bytes())
            .await?;
        outcome.files.created += files.created;
        outcome.files.updated += files.updated;
        outcome.files.removed += files.removed;
        outcome.lines.added += lines.added;
        outcome.lines.removed += lines.removed;
        for path in conflicts {
            if !outcome.conflicts.contains(&path) {
                outcome.conflicts.push(path);
            }
        }
        Ok(())
    }
}
