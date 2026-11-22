//! A work item's commit flow (`#workqueue` 11/14): its own commits, its **inbox** (what its line
//! ancestors have that it lacks, each attributed to the nearest ancestor carrying it, with a conflict
//! prediction made for *this* item) and its **outbox** (what it has that each successor lacks — always
//! its successors' inbox entries sourced from it).
//!
//! Read from remote tips (`origin/<branch>`) after a fetch: no local branch, index or working tree is
//! touched. Results are cached by the tips they were read at.
//!
//! `inbox(item)` = ⋃ over line ancestors `a` of `origin/<item>..origin/<a>`, trunk and external bases
//! listed first; `prediction` = `git merge-tree --write-tree` of the commit onto `origin/<item>` with the
//! commit's parent as merge base.

use std::path::PathBuf;

use crate::lineage::LineEndpoint;

/// An item resolved for commit analysis: its branch and its line parent (n9's lineage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemBranch {
    pub item_id: String,
    pub branch: String,
    pub line_parent: LineEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCommit {
    pub sha: String,
    pub subject: String,
}

/// Where an inbox commit comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitSource {
    /// The nearest line ancestor item whose branch carries it.
    Item(String),
    Trunk(String),
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prediction {
    Clean,
    /// The files it would conflict in.
    Conflicts(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    pub commit: FlowCommit,
    pub source: CommitSource,
    pub prediction: Prediction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    pub successor: String,
    pub commit: FlowCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFlowError(pub String);

/// Commit analysis over one clone (the queue owner's worktree), cached by remote tips.
pub struct CommitFlow {
    repo_dir: PathBuf,
}

impl CommitFlow {
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
        }
    }

    /// `git fetch origin --prune` — remote-tracking refs only.
    pub fn fetch(&self) -> Result<(), CommitFlowError> {
        // TODO(n11 commit-inbox-outbox): implement
        let _ = &self.repo_dir;
        todo!("TODO(n11): implement CommitFlow::fetch")
    }

    /// `item_id`'s own commits beyond its line parent, oldest first.
    pub fn own_commits(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<FlowCommit>, CommitFlowError> {
        // TODO(n11 commit-inbox-outbox): implement
        let _ = (queue, item_id);
        todo!("TODO(n11): implement CommitFlow::own_commits")
    }

    /// `item_id`'s inbox: trunk/external sources first, then items nearest first; oldest first within.
    pub fn inbox(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<InboxEntry>, CommitFlowError> {
        // TODO(n11 commit-inbox-outbox): implement
        let _ = (queue, item_id);
        todo!("TODO(n11): implement CommitFlow::inbox")
    }

    /// `item_id`'s outbox: per successor, the commits it lacks that `item_id` carries.
    pub fn outbox(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<OutboxEntry>, CommitFlowError> {
        // TODO(n11 commit-inbox-outbox): implement
        let _ = (queue, item_id);
        todo!("TODO(n11): implement CommitFlow::outbox")
    }

    /// How many reads were answered from the cache (unchanged tips).
    pub fn cache_hits(&self) -> u64 {
        // TODO(n11 commit-inbox-outbox): implement
        todo!("TODO(n11): implement CommitFlow::cache_hits")
    }
}
