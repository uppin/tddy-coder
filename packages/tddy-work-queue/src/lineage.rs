//! A work queue's lineage (`#workqueue` 9/14): two kinds of edge, never conflated.
//!
//! - **Line edges** — the branch chain on GitHub: an item's PR is based on another item's branch, on
//!   the trunk, or on a branch outside the queue. Read from the PR's base (through the inbox), never
//!   stored; an item without a PR shows its planned base (`parents`).
//! - **Dependency edges** — what needs what behaviour (`from → to`: `to` needs `from`). Stored on the
//!   queue, set by the agent or the user, acyclic; the only edges waves and readiness read.
//!
//! `parents` keeps its git meaning (where to cut and rebase); it is not migrated into dependencies.

use tddy_core::changeset::Stack;
use tddy_github_inbox::{GithubInbox, RepoSlug};

/// `to` needs `from`'s behaviour.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}

/// Where an item's line edge comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEndpoint {
    /// Another item of the queue (by id).
    Item(String),
    /// The trunk branch.
    Trunk(String),
    /// A branch outside the queue.
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineageError {
    NoSuchItem(String),
    /// The edges would close this cycle (item ids, first repeated last).
    Cycle(Vec<String>),
}

/// Which endpoint a PR based on `base_branch` hangs from, in `queue` with trunk `trunk`.
pub fn line_endpoint_of_base(queue: &Stack, base_branch: &str, trunk: &str) -> LineEndpoint {
    // TODO(n9 lineage-graph): implement
    let _ = (queue, base_branch, trunk);
    todo!("TODO(n9): implement line_endpoint_of_base")
}

/// An item without a PR: its planned base (first parent), or the trunk for a root.
pub fn planned_line_parent(
    queue: &Stack,
    item_id: &str,
    trunk: &str,
) -> Result<LineEndpoint, LineageError> {
    // TODO(n9 lineage-graph): implement
    let _ = (queue, item_id, trunk);
    todo!("TODO(n9): implement planned_line_parent")
}

/// Every item's line endpoint: its PR's base read through the inbox, else its planned base.
pub async fn line_edges_through_inbox(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &Stack,
    trunk: &str,
) -> Vec<(String, LineEndpoint)> {
    // TODO(n9 lineage-graph): implement
    let _ = (inbox, repo, queue, trunk);
    todo!("TODO(n9): implement line_edges_through_inbox")
}

/// Replace what `item_id` needs; refused when an item is unknown or the edges would close a cycle.
pub fn set_dependencies(
    queue: &mut Stack,
    item_id: &str,
    needs: Vec<String>,
) -> Result<(), LineageError> {
    // TODO(n9 lineage-graph): implement
    let _ = (queue, item_id, needs);
    todo!("TODO(n9): implement set_dependencies")
}

/// The queue's stored dependency edges, sorted.
pub fn dependencies_of(queue: &Stack) -> Vec<DependencyEdge> {
    // TODO(n9 lineage-graph): implement
    let _ = queue;
    todo!("TODO(n9): implement dependencies_of")
}

/// The cycle `edges` close, as the path that closes it; `None` when acyclic.
pub fn detect_cycle(edges: &[DependencyEdge]) -> Option<Vec<String>> {
    // TODO(n9 lineage-graph): implement
    let _ = edges;
    todo!("TODO(n9): implement detect_cycle")
}
