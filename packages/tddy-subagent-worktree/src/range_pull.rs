//! Handing a chosen range of a conversation's commits to the caller, commit by commit, skipping the
//! ones the caller already took.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::change_facts::{FileCounts, LineCounts};
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
    /// worktree are not touched. A bound that is not on the branch, or a `from` after `to`, is refused;
    /// a range with nothing left to pull applies nothing and is not an error.
    pub async fn pull_range(
        &self,
        range: &PullRange,
        already_pulled: &BTreeSet<String>,
    ) -> Result<RangePullOutcome, WorktreeError> {
        // TODO(range-pull): implement
        todo!("pull_range({range:?}, {already_pulled:?})")
    }
}
