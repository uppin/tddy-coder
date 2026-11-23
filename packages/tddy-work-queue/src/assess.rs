//! Node views: a queue's items with their child session states and live PR states, and the
//! effective base ref each one is rebased onto.

use std::path::Path;

use tddy_core::changeset::WorkQueue;
use tddy_core::workflow::ids::WorkflowState;

use tddy_github::pr_api::GithubPrApi;

/// Coarse phase of a single child PR node as seen by the orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildPhase {
    NotSpawned,
    SpawnRequested,
    Building,
    ReadyForPr,
    PrOpen,
    Failed(String),
}

/// Live GitHub PR status for one stack node branch.
#[derive(Debug, Clone, PartialEq)]
pub enum PrLiveStatus {
    None,
    Open { number: u64, base: String },
    Queued,
    Merged,
    Closed,
}

/// Snapshot of one stack node as seen during an assess tick.
#[derive(Debug, Clone)]
pub struct NodeView {
    pub node_id: String,
    pub branch: String,
    pub parent_dep_ids: Vec<String>,
    pub child_session_id: Option<String>,
    pub child_state: Option<WorkflowState>,
    pub child_phase: ChildPhase,
    pub pr: PrLiveStatus,
}

/// Effective base ref for a node (skips merged ancestors; returns default_branch when all merged).
pub fn effective_base_ref(node_id: &str, views: &[NodeView], default_branch: &str) -> String {
    let Some(node) = views.iter().find(|v| v.node_id == node_id) else {
        return default_branch.to_string();
    };
    if node.parent_dep_ids.is_empty() {
        return default_branch.to_string();
    }
    // Walk each parent; if all are merged, collapse to default_branch.
    // Otherwise return the branch of the nearest unmerged parent. A parent whose branch is not
    // known yet names no ref, so it is skipped exactly like an absent one — same rule as
    // `tddy_core::changeset::WorkQueue::effective_base_refs`.
    for parent_id in &node.parent_dep_ids {
        let Some(parent_view) = views.iter().find(|v| &v.node_id == parent_id) else {
            continue;
        };
        if parent_view.branch.is_empty() {
            continue;
        }
        if !matches!(parent_view.pr, PrLiveStatus::Merged) {
            return parent_view.branch.clone();
        }
    }
    // All parents are merged → use default_branch
    default_branch.to_string()
}

/// Assemble NodeView list from orchestrator changeset + child changesets + live GitHub.
pub fn assemble_views(
    parent_dir: &Path,
    sessions_root: &Path,
    stack: &WorkQueue,
    gh: &dyn GithubPrApi,
    _default_branch: &str,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    assemble_views_with(parent_dir, sessions_root, stack, |branch| {
        Ok(gh
            .get_open_pr(branch)?
            .map(|pr| (pr.number, pr.base_branch)))
    })
}

/// [`assemble_views`] with the open-PR lookup supplied: `open_pr_of(branch)` answers the open PR's
/// `(number, base branch)` for a node's resolved branch, `None` when it has none. The lookup is the
/// only part that reads GitHub, so a caller that reads through the inbox answers it from rows it has
/// already fetched (see `inbox_reads`).
pub(crate) fn assemble_views_with(
    parent_dir: &Path,
    sessions_root: &Path,
    stack: &WorkQueue,
    mut open_pr_of: impl FnMut(&str) -> Result<Option<(u64, String)>, tddy_core::WorkflowError>,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    use tddy_core::changeset::{read_changeset, resolve_stack_node_branch};
    use tddy_core::session_lifecycle::unified_session_dir_path;

    let order = stack.topo_order()?;

    let mut views: Vec<NodeView> = Vec::with_capacity(stack.nodes.len());

    for node_id in &order {
        let node = stack
            .nodes
            .iter()
            .find(|n| &n.node_id == node_id)
            .expect("topo_order only contains node ids from the stack");

        // The branch a node stands on, hydrated from its child session when the node itself never
        // recorded one. Empty means "this node owns no branch yet" — never a fabricated name, which
        // would make the view claim a ref that does not exist.
        let branch = resolve_stack_node_branch(sessions_root, node).unwrap_or_default();

        let (child_state, child_phase) = if let Some(ref session_id) = node.session_id {
            let child_dir = unified_session_dir_path(sessions_root, session_id);
            match read_changeset(&child_dir) {
                Ok(cs) => {
                    let state = cs.state.current.clone();
                    let phase = workflow_state_to_child_phase(&state);
                    (Some(state), phase)
                }
                Err(_) => (None, ChildPhase::Building),
            }
        } else {
            let spawn_request_marker = parent_dir
                .join(".workflow")
                .join(format!("spawn-request-{node_id}.json"));
            if spawn_request_marker.exists() {
                (None, ChildPhase::SpawnRequested)
            } else {
                (None, ChildPhase::NotSpawned)
            }
        };

        // A PR belongs to the branch, not to the child session that happened to create it: report
        // it for every node that owns a branch, whether or not a session is still attached.
        let pr = if !branch.is_empty() {
            match open_pr_of(&branch)? {
                Some((number, base)) => PrLiveStatus::Open { number, base },
                None => {
                    if let Some(ref status) = node.pr_status {
                        match status.phase.as_str() {
                            "merged" => PrLiveStatus::Merged,
                            "closed" => PrLiveStatus::Closed,
                            _ => PrLiveStatus::None,
                        }
                    } else {
                        PrLiveStatus::None
                    }
                }
            }
        } else {
            PrLiveStatus::None
        };

        views.push(NodeView {
            node_id: node_id.clone(),
            branch,
            parent_dep_ids: node.parents.clone(),
            child_session_id: node.session_id.clone(),
            child_state,
            child_phase,
            pr,
        });
    }

    Ok(views)
}

fn workflow_state_to_child_phase(state: &tddy_core::workflow::ids::WorkflowState) -> ChildPhase {
    match state.as_str() {
        "Done" | "End" => ChildPhase::ReadyForPr,
        "Failed" | "Error" => ChildPhase::Failed(state.to_string()),
        _ => ChildPhase::Building,
    }
}

#[cfg(test)]
mod tests;
