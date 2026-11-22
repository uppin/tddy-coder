//! A work queue's lineage (`#workqueue` 9/14): two kinds of edge, never conflated.
//!
//! - **Line edges** — the branch chain on GitHub: an item's PR is based on another item's branch, on
//!   the trunk, or on a branch outside the queue. Read from the PR's base (through the inbox), never
//!   stored; an item without a PR shows its planned base (`parents`).
//! - **Dependency edges** — what needs what behaviour (`from → to`: `to` needs `from`). Stored on the
//!   queue, set by the agent or the user, acyclic; the only edges waves and readiness read.
//!
//! `parents` keeps its git meaning (where to cut and rebase); it is not migrated into dependencies.

use std::collections::{BTreeMap, BTreeSet};

use tddy_core::changeset::WorkQueue;
use tddy_github::pr_api::PrState;
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

impl LineEndpoint {
    /// The wire's kind of this endpoint: `item`, `trunk` or `external`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Item(_) => "item",
            Self::Trunk(_) => "trunk",
            Self::External(_) => "external",
        }
    }

    /// The item id, or the branch name.
    pub fn name(&self) -> &str {
        match self {
            Self::Item(name) | Self::Trunk(name) | Self::External(name) => name,
        }
    }
}

/// A queue's whole lineage: each item's line endpoint and the stored dependency edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueGraph {
    pub line: Vec<(String, LineEndpoint)>,
    pub dependencies: Vec<DependencyEdge>,
}

impl std::fmt::Display for LineageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchItem(id) => write!(f, "item '{id}' not found"),
            Self::Cycle(path) => write!(
                f,
                "these dependencies would close a cycle: {}",
                path.join(" -> ")
            ),
        }
    }
}

/// The trunk's branch name from the project's integration base ref (`origin/main` → `main`).
pub fn trunk_branch_of(integration_base_ref: &str) -> &str {
    tddy_git::local_branch_name_for_remote(integration_base_ref, "origin")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineageError {
    NoSuchItem(String),
    /// The edges would close this cycle (item ids, first repeated last).
    Cycle(Vec<String>),
}

/// Which endpoint a PR based on `base_branch` hangs from, in `queue` with trunk `trunk`.
pub fn line_endpoint_of_base(queue: &WorkQueue, base_branch: &str, trunk: &str) -> LineEndpoint {
    if base_branch == trunk {
        return LineEndpoint::Trunk(trunk.to_string());
    }
    match queue
        .nodes
        .iter()
        .find(|item| item.branch.as_deref() == Some(base_branch))
    {
        Some(item) => LineEndpoint::Item(item.node_id.clone()),
        None => LineEndpoint::External(base_branch.to_string()),
    }
}

/// An item without a PR: its planned base (first parent), or the trunk for a root.
pub fn planned_line_parent(
    queue: &WorkQueue,
    item_id: &str,
    trunk: &str,
) -> Result<LineEndpoint, LineageError> {
    let item = queue
        .node(item_id)
        .ok_or_else(|| LineageError::NoSuchItem(item_id.to_string()))?;
    match item.parents.first() {
        None => Ok(LineEndpoint::Trunk(trunk.to_string())),
        Some(parent) if queue.node(parent).is_some() => Ok(LineEndpoint::Item(parent.clone())),
        Some(parent) => Err(LineageError::NoSuchItem(parent.clone())),
    }
}

/// Every item's line endpoint: its PR's base read through the inbox, else its planned base.
///
/// An item whose planned base names an unknown item is left out (there is no endpoint to draw).
pub async fn line_edges_through_inbox(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
    trunk: &str,
) -> Vec<(String, LineEndpoint)> {
    let mut edges = Vec::with_capacity(queue.nodes.len());
    for item in &queue.nodes {
        let from_pr = match item.branch.as_deref().filter(|b| !b.is_empty()) {
            Some(branch) => match inbox.pr_for_head(repo, branch).await {
                Ok(cached) => cached
                    .value
                    .filter(|pr| matches!(pr.state, PrState::Open | PrState::Draft))
                    .map(|pr| line_endpoint_of_base(queue, &pr.base_branch, trunk)),
                // An unread PR is not "no PR": the planned base stands until the inbox answers.
                Err(error) => {
                    log::debug!("lineage: PR of {branch} unreadable: {error:?}");
                    None
                }
            },
            None => None,
        };
        let endpoint = from_pr.or_else(|| planned_line_parent(queue, &item.node_id, trunk).ok());
        if let Some(endpoint) = endpoint {
            edges.push((item.node_id.clone(), endpoint));
        }
    }
    edges
}

/// Every item's planned line endpoint — what the line shows when no PR can be read.
pub fn planned_line_edges(queue: &WorkQueue, trunk: &str) -> Vec<(String, LineEndpoint)> {
    queue
        .nodes
        .iter()
        .filter_map(|item| {
            planned_line_parent(queue, &item.node_id, trunk)
                .ok()
                .map(|endpoint| (item.node_id.clone(), endpoint))
        })
        .collect()
}

/// The queue's lineage: line edges through the inbox, dependency edges as stored.
pub async fn queue_graph(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
    trunk: &str,
) -> QueueGraph {
    QueueGraph {
        line: line_edges_through_inbox(inbox, repo, queue, trunk).await,
        dependencies: dependencies_of(queue),
    }
}

/// Replace what `item_id` needs; refused when an item is unknown or the edges would close a cycle.
/// On refusal the queue is unchanged.
pub fn set_dependencies(
    queue: &mut WorkQueue,
    item_id: &str,
    needs: Vec<String>,
) -> Result<(), LineageError> {
    if queue.node(item_id).is_none() {
        return Err(LineageError::NoSuchItem(item_id.to_string()));
    }
    if let Some(unknown) = needs.iter().find(|need| queue.node(need).is_none()) {
        return Err(LineageError::NoSuchItem(unknown.clone()));
    }
    let mut needs = needs;
    needs.dedup();
    let mut candidate = queue.clone();
    if let Some(item) = candidate.nodes.iter_mut().find(|n| n.node_id == item_id) {
        item.needs = needs;
    }
    if let Some(cycle) = detect_cycle(&dependencies_of(&candidate)) {
        return Err(LineageError::Cycle(cycle));
    }
    *queue = candidate;
    Ok(())
}

/// The queue's stored dependency edges, sorted.
pub fn dependencies_of(queue: &WorkQueue) -> Vec<DependencyEdge> {
    let mut edges: Vec<DependencyEdge> = queue
        .nodes
        .iter()
        .flat_map(|item| {
            item.needs.iter().map(|need| DependencyEdge {
                from: need.clone(),
                to: item.node_id.clone(),
            })
        })
        .collect();
    edges.sort();
    edges.dedup();
    edges
}

/// The cycle `edges` close, as the path that closes it (first node repeated last); `None` when
/// acyclic. Deterministic: nodes and successors are visited in sorted order.
pub fn detect_cycle(edges: &[DependencyEdge]) -> Option<Vec<String>> {
    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        successors
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
        successors.entry(edge.to.as_str()).or_default();
    }
    for targets in successors.values_mut() {
        targets.sort_unstable();
        targets.dedup();
    }
    let mut done: BTreeSet<&str> = BTreeSet::new();
    for &start in successors.keys() {
        if done.contains(start) {
            continue;
        }
        let mut path: Vec<&str> = vec![start];
        let mut next_child: Vec<usize> = vec![0];
        while let Some(&node) = path.last() {
            let depth = path.len() - 1;
            let child = successors[node].get(next_child[depth]).copied();
            match child {
                None => {
                    done.insert(node);
                    path.pop();
                    next_child.pop();
                }
                Some(child) => {
                    next_child[depth] += 1;
                    if let Some(at) = path.iter().position(|&p| p == child) {
                        let mut cycle: Vec<String> =
                            path[at..].iter().map(|s| s.to_string()).collect();
                        cycle.push(child.to_string());
                        return Some(cycle);
                    }
                    if !done.contains(child) {
                        path.push(child);
                        next_child.push(0);
                    }
                }
            }
        }
    }
    None
}
