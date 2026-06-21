//! Assess task: reads the stack DAG + child session states + GitHub PR states,
//! then decides the next orchestrator action.

use std::path::Path;

use async_trait::async_trait;
use tddy_core::changeset::Stack;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{Task, TaskResult, NextAction};

use super::github::GithubPrApi;

/// Coarse phase of a single child PR node as seen by the orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildPhase {
    NotSpawned,
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

/// The orchestrator's decision for this tick.
#[derive(Debug, Clone, PartialEq)]
pub enum OrchestratorAction {
    Spawn { node_ids: Vec<String> },
    Wait { reason: String },
    Merge { node_id: String, pr_number: u64 },
    MarkFailed { node_id: String, reason: String },
    Done,
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

/// Pure decision function: given assembled views, pick the next orchestrator action.
///
/// `autonomous_merge`: when `false`, the orchestrator waits for an operator gate before merging.
/// `approved_nodes`: set of node IDs that the operator has explicitly approved for merge
///   (only consulted when `autonomous_merge` is `false`).
pub fn decide_next_action(
    views: &[NodeView],
    autonomous_merge: bool,
    approved_nodes: &std::collections::HashSet<String>,
) -> OrchestratorAction {
    // A node is considered "complete" (merged) when its PR is merged.
    let is_merged = |v: &NodeView| matches!(v.pr, PrLiveStatus::Merged);
    // A node's dependencies are satisfied for merge when every parent dep is merged.
    let deps_merged = |v: &NodeView| {
        v.parent_dep_ids.iter().all(|dep| {
            views
                .iter()
                .find(|other| &other.node_id == dep)
                .map(is_merged)
                .unwrap_or(false)
        })
    };
    // A node's dependencies are satisfied for spawning when every parent dep has
    // already been spawned (i.e. is not still NotSpawned).
    let deps_spawned = |v: &NodeView| {
        v.parent_dep_ids.iter().all(|dep| {
            views
                .iter()
                .find(|other| &other.node_id == dep)
                .map(|other| other.child_phase != ChildPhase::NotSpawned)
                .unwrap_or(false)
        })
    };

    // 1. Any failed node or closed PR → MarkFailed.
    for v in views {
        if let ChildPhase::Failed(reason) = &v.child_phase {
            return OrchestratorAction::MarkFailed {
                node_id: v.node_id.clone(),
                reason: reason.clone(),
            };
        }
        if matches!(v.pr, PrLiveStatus::Closed) {
            return OrchestratorAction::MarkFailed {
                node_id: v.node_id.clone(),
                reason: format!("PR for node {} was closed without merging", v.node_id),
            };
        }
    }

    // 2. Done when every node is merged.
    if !views.is_empty() && views.iter().all(is_merged) {
        return OrchestratorAction::Done;
    }

    // 3. Spawn the first node (bottom→top) that hasn't been spawned and whose deps are spawned.
    for v in views {
        if v.child_phase == ChildPhase::NotSpawned && deps_spawned(v) {
            return OrchestratorAction::Spawn {
                node_ids: vec![v.node_id.clone()],
            };
        }
    }

    // 4. Merge an open PR whose deps are merged, subject to the merge gate.
    for v in views {
        if let PrLiveStatus::Open { number, .. } = &v.pr {
            if deps_merged(v) {
                if autonomous_merge || approved_nodes.contains(&v.node_id) {
                    return OrchestratorAction::Merge {
                        node_id: v.node_id.clone(),
                        pr_number: *number,
                    };
                }
                return OrchestratorAction::Wait {
                    reason: format!(
                        "node {} is ready to merge but awaiting operator approval",
                        v.node_id
                    ),
                };
            }
        }
    }

    // 5/6. Otherwise wait for in-flight work.
    OrchestratorAction::Wait {
        reason: "waiting for child sessions / PRs to progress".to_string(),
    }
}

// TODO: implement — consolidate with Stack::effective_base_refs, deriving single-ref from NodeView slice
/// Effective base ref for a node (skips merged ancestors; returns default_branch when all merged).
pub fn effective_base_ref(node_id: &str, views: &[NodeView], default_branch: &str) -> String {
    unimplemented!("effective_base_ref: not yet implemented")
}

// TODO: implement — reads child changesets via unified_session_dir_path + read_changeset; calls GithubPrApi::get_open_pr
/// Assemble NodeView list from orchestrator changeset + child changesets + live GitHub.
pub fn assemble_views(
    parent_dir: &Path,
    sessions_root: &Path,
    stack: &Stack,
    gh: &dyn GithubPrApi,
    default_branch: &str,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    unimplemented!("assemble_views: not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_view(node_id: &str, parents: &[&str], phase: ChildPhase, pr: PrLiveStatus) -> NodeView {
        NodeView {
            node_id: node_id.to_string(),
            branch: format!("feature/{node_id}"),
            parent_dep_ids: parents.iter().map(|s| s.to_string()).collect(),
            child_session_id: None,
            child_state: None,
            child_phase: phase,
            pr,
        }
    }

    #[test]
    fn decide_next_action_mark_failed_when_node_failed() {
        let views = vec![node_view(
            "n1",
            &[],
            ChildPhase::Failed("rebase conflict".to_string()),
            PrLiveStatus::None,
        )];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        assert!(
            matches!(action, OrchestratorAction::MarkFailed { ref node_id, .. } if node_id == "n1"),
            "expected MarkFailed for failed node, got: {action:?}"
        );
    }

    #[test]
    fn decide_next_action_spawn_root_node_not_yet_spawned() {
        let views = vec![node_view("n1", &[], ChildPhase::NotSpawned, PrLiveStatus::None)];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        assert!(
            matches!(&action, OrchestratorAction::Spawn { node_ids } if node_ids == &["n1".to_string()]),
            "expected Spawn for unspawned root, got: {action:?}"
        );
    }

    #[test]
    fn decide_next_action_wait_when_node_building() {
        let mut view = node_view("n1", &[], ChildPhase::Building, PrLiveStatus::None);
        view.child_session_id = Some("sess-1".to_string());
        let views = vec![view];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        assert!(
            matches!(action, OrchestratorAction::Wait { .. }),
            "expected Wait when node is Building, got: {action:?}"
        );
    }

    #[test]
    fn decide_next_action_merge_when_pr_open_and_deps_merged() {
        let n1 = node_view("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged);
        let n2 = node_view(
            "n2",
            &["n1"],
            ChildPhase::PrOpen,
            PrLiveStatus::Open { number: 7, base: "master".to_string() },
        );
        let action = decide_next_action(&[n1, n2], true, &std::collections::HashSet::new());
        assert!(
            matches!(&action, OrchestratorAction::Merge { node_id, pr_number } if node_id == "n2" && *pr_number == 7),
            "expected Merge for n2, got: {action:?}"
        );
    }

    #[test]
    fn decide_next_action_done_when_all_merged() {
        let n1 = node_view("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged);
        let n2 = node_view("n2", &["n1"], ChildPhase::PrOpen, PrLiveStatus::Merged);
        let action = decide_next_action(&[n1, n2], false, &std::collections::HashSet::new());
        assert_eq!(action, OrchestratorAction::Done, "expected Done when all merged");
    }

    #[test]
    fn decide_next_action_wait_for_merge_gate_when_autonomous_merge_off() {
        // One node: deps merged, PR open and base is already master — normally ready to merge.
        // With autonomous_merge = false (the default), decide_next_action should return Wait.
        let views = vec![node_view(
            "n1",
            &[],
            ChildPhase::PrOpen,
            PrLiveStatus::Open { number: 3, base: "master".to_string() },
        )];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        // Gate-off default: expect Wait, not Merge
        assert!(
            matches!(action, OrchestratorAction::Wait { .. }),
            "expected Wait when merge gate is off (operator-gated default), got: {action:?}"
        );
    }
}

// TODO: implement AssessTask::run — recover_in_flight_stack_op → assemble_views → decide_next_action → GoTo
/// The assess Task: reads stack, calls assemble_views + decide_next_action, returns GoTo.
pub struct AssessTask {}

impl AssessTask {
    pub fn new() -> Self { Self {} }
}

impl Default for AssessTask {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Task for AssessTask {
    fn id(&self) -> &str { "assess" }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!("AssessTask::run: not yet implemented")
    }
}
