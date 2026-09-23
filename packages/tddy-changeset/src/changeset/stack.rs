//! The PR-stack DAG an orchestrator session carries, and the operations that maintain it.
//!
//! A data model of its own, unrelated to [`super::model::Changeset`] beyond being stored
//! alongside it — which is why it was the first seam the 964-line file offered.

use super::io::{read_changeset, write_changeset_atomic};
use super::model::{GithubPrStatus, PrInternalStatus};
use crate::error::WorkflowError;
use std::collections::BTreeMap;
use std::path::Path;
use tddy_workflow::ids::WorkflowState;

/// PR-stack DAG carried by the ORCHESTRATOR session's changeset.
/// Each node is a child PR session reference.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stack {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub nodes: Vec<StackNode>,
}

/// A single node in the PR-stack DAG.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StackNode {
    /// Stable planner-assigned id (e.g. "n1"). Exists before a child session is materialized.
    pub node_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Suggested git branch name (e.g. "feature/auth-token-store").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_suggestion: Option<String>,
    /// Actual branch once the child worktree is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Child session id once materialized; None while only planned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Parent NODE ids (not session ids). Empty = root off stack base; >1 = DAG.
    #[serde(default)]
    pub parents: Vec<String>,
    /// Reuses existing GithubPrStatus; phase values: "planned"|"open"|"merged"|"closed"|"error".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_status: Option<GithubPrStatus>,
    /// Coarse mirror of child session WorkflowState for orchestrator dashboards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_state: Option<WorkflowState>,
    /// Action-needed signal, orthogonal to `pr_status` (which mirrors GitHub reality).
    /// Auto-derived from git + GitHub, or set by the agent as an override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_status: Option<PrInternalStatus>,
    /// Where this node reads in the operator's list — a fact independent of `parents`.
    ///
    /// The dependency graph says which node builds on which; this says how the operator wants the
    /// rows laid out. Deriving the second from the first means a merge, a repoint or a re-parenting
    /// silently moves a row nobody touched. `None` is a stack written before the field existed; the
    /// next write to that stack numbers every node (see `Stack::display_order`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_order: Option<u32>,
}

impl StackNode {
    /// A node is "skipped" when its PR has been merged.
    pub fn is_skipped(&self) -> bool {
        self.pr_status
            .as_ref()
            .map(|s| s.phase == "merged")
            .unwrap_or(false)
    }
}

impl Stack {
    /// Find a node by node_id.
    pub fn node(&self, node_id: &str) -> Option<&StackNode> {
        self.nodes.iter().find(|n| n.node_id == node_id)
    }

    /// Kahn topological sort over node_id/parents relationships.
    /// Returns ordered list of node_ids (leaves last). Cycle → WorkflowError::ChangesetInvalid.
    pub fn topo_order(&self) -> Result<Vec<String>, WorkflowError> {
        use std::collections::VecDeque;

        // In-degree = number of parents that exist as nodes in this stack.
        let known: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.node_id.as_str()).collect();
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        // children[parent] = nodes that depend on `parent`.
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &self.nodes {
            in_degree.entry(node.node_id.as_str()).or_insert(0);
            for parent in &node.parents {
                if known.contains(parent.as_str()) {
                    *in_degree.entry(node.node_id.as_str()).or_insert(0) += 1;
                    children
                        .entry(parent.as_str())
                        .or_default()
                        .push(node.node_id.as_str());
                }
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut order: Vec<String> = Vec::with_capacity(self.nodes.len());
        while let Some(id) = queue.pop_front() {
            order.push(id.to_string());
            if let Some(deps) = children.get(id) {
                for &child in deps {
                    let d = in_degree.get_mut(child).expect("child has in-degree entry");
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            return Err(WorkflowError::ChangesetInvalid(format!(
                "cycle detected in PR-stack DAG: only {} of {} nodes orderable",
                order.len(),
                self.nodes.len()
            )));
        }
        Ok(order)
    }

    /// The order the stack's rows are *read* in: node ids sorted by their persisted
    /// [`StackNode::display_order`], falling back to [`Stack::topo_order`] for the nodes that carry
    /// none.
    ///
    /// Never fails, because this is a render path: a list the operator cannot see is a worse answer
    /// than a list in an imperfect order. A cycle — which [`Stack::topo_order`] refuses — degrades
    /// the tie-break to the order the nodes are declared in, and no node is ever dropped.
    ///
    /// The sort key is `(display_order, topological index, node id)`. So a stack where every node is
    /// numbered reads strictly in the persisted order; a stack where none is reads exactly what
    /// [`Stack::topo_order`] returns; and a half-numbered stack — which has no coherent total order
    /// of its own — puts the numbered rows first, in their numbers, and the rest behind them
    /// topologically.
    ///
    /// That half-numbered case is an internal invariant of this sort key rather than a state any
    /// caller reaches: every write numbers whatever it finds unnumbered, and this function's only
    /// production caller runs after that numbering inside the same atomic update. The web
    /// deliberately does **not** mirror it — `orderStackNodes` falls back to topological order
    /// *wholesale* for a half-numbered plan (D25), because interleaving real positions with invented
    /// ones can render a child above its parent. Aligning the two would be a decision, not a tidy-up.
    #[must_use]
    pub fn display_order(&self) -> Vec<String> {
        let topo_index: BTreeMap<String, usize> = self
            .topo_order()
            .map(|order| order.into_iter().zip(0..).collect())
            .unwrap_or_default();

        let mut ordered: Vec<&StackNode> = self.nodes.iter().collect();
        ordered.sort_by_key(|node| {
            (
                node.display_order.unwrap_or(u32::MAX),
                // A cycle leaves `topo_index` empty, so every node falls back to where it is
                // declared — an order, rather than an error the caller cannot render.
                topo_index
                    .get(&node.node_id)
                    .copied()
                    .unwrap_or_else(|| self.declaration_index_of(&node.node_id)),
                node.node_id.as_str(),
            )
        });
        ordered.iter().map(|n| n.node_id.clone()).collect()
    }

    /// Where a node sits in the `nodes` array — the tie-break [`Stack::display_order`] falls back to
    /// when the DAG cannot be sorted.
    fn declaration_index_of(&self, node_id: &str) -> usize {
        self.nodes
            .iter()
            .position(|n| n.node_id == node_id)
            .unwrap_or(usize::MAX)
    }

    /// Effective base origin refs for a node, skipping merged ancestors.
    /// Returns `origin/<branch>` for each nearest non-skipped ancestor across all `parents`,
    /// or `[stack_bottom_base.to_string()]` when all parents are merged/absent.
    ///
    /// Only a parent that owns a real `branch` contributes a ref: a branch is the only thing a
    /// worktree can be created from, and a node id (or a planner's `branch_suggestion`) names no
    /// ref that exists. Use [`Stack::base_ref_for_spawn`] when a branchless parent must be a hard
    /// refusal rather than a silent omission.
    pub fn effective_base_refs(&self, node_id: &str, stack_bottom_base: &str) -> Vec<String> {
        let Some(node) = self.node(node_id) else {
            return vec![stack_bottom_base.to_string()];
        };
        let refs: Vec<String> = node
            .parents
            .iter()
            .filter_map(|parent_id| self.node(parent_id))
            .filter(|parent| !parent.is_skipped())
            .filter_map(|parent| parent.branch.as_deref())
            .map(|branch| format!("origin/{branch}"))
            .collect();
        if refs.is_empty() {
            vec![stack_bottom_base.to_string()]
        } else {
            refs
        }
    }

    /// Resolve the base ref a child worktree for `node_id` must branch off.
    ///
    /// Enforces bottom-up ordering first: if any non-merged parent owns no `branch`, spawning is
    /// refused with a `ChangesetInvalid` error naming that parent, because there is no ref to base
    /// onto. Merged parents are skipped, not required. A parent's child *session* plays no part in
    /// this decision — a branch can be built on whether or not a session is still attached to it
    /// (see [`resolve_stack_node_branch`], which hydrates a node's branch from its child session
    /// when the node itself never recorded one).
    ///
    /// Otherwise returns the nearest non-merged ancestor's `origin/<branch>` (the first
    /// `effective_base_refs` entry), or `stack_bottom_base` for a root node or a node whose
    /// parents are all merged.
    pub fn base_ref_for_spawn(
        &self,
        node_id: &str,
        stack_bottom_base: &str,
    ) -> Result<String, WorkflowError> {
        if let Some(node) = self.node(node_id) {
            for parent_id in &node.parents {
                if let Some(parent) = self.node(parent_id) {
                    if !parent.is_skipped() && parent.branch.is_none() {
                        return Err(WorkflowError::ChangesetInvalid(format!(
                            "cannot spawn node '{node_id}': non-merged parent '{parent_id}' has no branch to base onto yet"
                        )));
                    }
                }
            }
        }

        let refs = self.effective_base_refs(node_id, stack_bottom_base);
        Ok(refs
            .into_iter()
            .next()
            .unwrap_or_else(|| stack_bottom_base.to_string()))
    }
}

/// Atomically update the stack on an orchestrator session.
pub fn update_stack_atomic(
    orchestrator_session_dir: &Path,
    f: impl FnOnce(&mut Stack),
) -> Result<(), WorkflowError> {
    let mut changeset = read_changeset(orchestrator_session_dir)?;
    let stack = changeset.stack.get_or_insert_with(Stack::default);
    f(stack);
    write_changeset_atomic(orchestrator_session_dir, &changeset)
}

/// Link a stack node to its materialized child session (set session_id + branch).
pub fn link_stack_node_to_child_session(
    orchestrator_session_dir: &Path,
    node_id: &str,
    child_session_id: &str,
    branch: Option<String>,
) -> Result<(), WorkflowError> {
    update_stack_atomic(orchestrator_session_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.session_id = Some(child_session_id.to_string());
            if let Some(b) = branch {
                node.branch = Some(b);
            }
        }
    })
}

/// Resolve the branch a stack node stands on: the branch the node recorded itself, falling back
/// to the branch recorded by its child session's changeset.
///
/// The node's own record is authoritative — the session is only a fallback route to the same
/// answer, for a node linked before its branch was known (or an older manifest). A node with no
/// branch and no reachable session resolves to `None`; a session directory that is gone from disk
/// is `None` too, never an error, because a deleted session must not un-resolve stack progression.
pub fn resolve_stack_node_branch(sessions_root: &Path, node: &StackNode) -> Option<String> {
    if let Some(branch) = node.branch.clone() {
        return Some(branch);
    }
    let session_id = node.session_id.as_deref()?;
    let child_dir = crate::session_lifecycle::unified_session_dir_path(sessions_root, session_id);
    read_changeset(&child_dir).ok()?.branch
}

/// Read an orchestrator session's stack with every node's `branch` hydrated via
/// [`resolve_stack_node_branch`], so branch-gated decisions (spawn ordering, PR lookup) see the
/// branch a child session created even when the node itself never recorded it.
///
/// `Ok(None)` when the session carries no stack (an ordinary, non-orchestrator session).
pub fn read_stack_with_resolved_branches(
    sessions_root: &Path,
    orchestrator_session_id: &str,
) -> Result<Option<Stack>, WorkflowError> {
    let dir =
        crate::session_lifecycle::unified_session_dir_path(sessions_root, orchestrator_session_id);
    let Some(mut stack) = read_changeset(&dir)?.stack else {
        return Ok(None);
    };
    for node in &mut stack.nodes {
        node.branch = resolve_stack_node_branch(sessions_root, node);
    }
    Ok(Some(stack))
}

/// Sync a stack node's child_state + pr_status from the child session's changeset.
/// Reads child via `unified_session_dir_path(sessions_root, node.session_id)` + `read_changeset`.
pub fn sync_stack_node_from_child(
    orchestrator_session_dir: &Path,
    sessions_root: &Path,
    node_id: &str,
) -> Result<(), WorkflowError> {
    // Resolve the child session id from the current orchestrator stack before any write.
    let orch = read_changeset(orchestrator_session_dir)?;
    let session_id = orch
        .stack
        .as_ref()
        .and_then(|s| s.node(node_id))
        .and_then(|n| n.session_id.clone());
    let Some(session_id) = session_id else {
        return Ok(());
    };

    let child_dir = crate::session_lifecycle::unified_session_dir_path(sessions_root, &session_id);
    let child = read_changeset(&child_dir)?;
    let child_state = child.state.current.clone();
    let pr_status = child
        .workflow
        .as_ref()
        .and_then(|w| w.github_pr_status.clone());

    update_stack_atomic(orchestrator_session_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.child_state = Some(child_state);
            node.pr_status = pr_status;
        }
    })
}

#[cfg(test)]
mod stack_tests {
    use super::*;
    use crate::changeset::io::write_changeset;
    use crate::changeset::model::{Changeset, ChangesetWorkflow};

    #[test]
    fn effective_base_refs_single_parent_skip() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "merged".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n2", "origin/master");
        assert_eq!(refs, vec!["origin/master".to_string()]);
    }

    #[test]
    fn effective_base_refs_multi_parent_unmerged_set() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "open".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: Some("sess-2".to_string()),
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "open".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n3".to_string()),
                    session_id: None,
                    parents: vec!["n1".to_string(), "n2".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n3", "origin/master");
        assert_eq!(
            refs,
            vec![
                "origin/feature/n1".to_string(),
                "origin/feature/n2".to_string()
            ]
        );
    }

    #[test]
    fn effective_base_refs_all_merged_returns_bottom_base() {
        let merged_status = || {
            Some(GithubPrStatus {
                phase: "merged".to_string(),
                url: None,
                error: None,
            })
        };
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some("sess-1".to_string()),
                    parents: vec![],
                    pr_status: merged_status(),
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n2".to_string()),
                    session_id: Some("sess-2".to_string()),
                    parents: vec!["n1".to_string()],
                    pr_status: merged_status(),
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n3".to_string()),
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
            ],
        };
        let refs = stack.effective_base_refs("n3", "origin/master");
        assert_eq!(refs, vec!["origin/master".to_string()]);
    }

    #[test]
    fn topo_order_linear_dag() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n3".to_string(),
                    title: "Node 3".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
            ],
        };
        let order = stack.topo_order().unwrap();
        assert_eq!(
            order,
            vec!["n1".to_string(), "n2".to_string(), "n3".to_string()]
        );
    }

    #[test]
    fn topo_order_cycle_returns_error() {
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n2".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
                StackNode {
                    node_id: "n2".to_string(),
                    title: "Node 2".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n1".to_string()],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                },
            ],
        };
        let result = stack.topo_order();
        assert!(result.is_err(), "expected Err for cycle, got Ok");
        let err = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err.contains("cycle"),
            "error message should mention 'cycle', got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // `display_order` — the *reading* order, which is a different fact from the DAG
    // -----------------------------------------------------------------------

    /// A node with the stated id, parents and recorded position, and nothing else set.
    fn a_node(node_id: &str, parents: &[&str], display_order: Option<u32>) -> StackNode {
        StackNode {
            node_id: node_id.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            display_order,
            ..StackNode::default()
        }
    }

    fn a_stack(nodes: Vec<StackNode>) -> Stack {
        Stack { version: 1, nodes }
    }

    #[test]
    fn display_order_reads_a_fully_numbered_stack_in_its_recorded_positions() {
        // Given — a chain whose recorded reading order deliberately contradicts the DAG: the
        // operator moved the leaf above the root, and where a row reads is not what it builds on
        let stack = a_stack(vec![
            a_node("n1", &[], Some(2)),
            a_node("n2", &["n1"], Some(1)),
            a_node("n3", &["n2"], Some(0)),
        ]);

        // When
        let order = stack.display_order();

        // Then
        assert_eq!(order, vec!["n3", "n2", "n1"]);
    }

    #[test]
    fn display_order_falls_back_to_topological_order_for_a_stack_that_records_none() {
        // Given — a plan written before display order existed, stored in the `nodes` array in
        // reverse: the array has never been ordered by anything
        let stack = a_stack(vec![
            a_node("n3", &["n2"], None),
            a_node("n2", &["n1"], None),
            a_node("n1", &[], None),
        ]);

        // When
        let order = stack.display_order();

        // Then — byte-identical to what `topo_order` returns, roots before dependents
        assert_eq!(order, stack.topo_order().unwrap());
        assert_eq!(order, vec!["n1", "n2", "n3"]);
    }

    #[test]
    fn a_half_numbered_stack_sorts_its_numbered_rows_first_rather_than_interleaving_them() {
        // Given — a half-numbered stack, which has no coherent total order of its own.
        //
        // This is an internal invariant of the sort key, not what the operator ever sees. The only
        // production caller of `display_order` is the PR-stack recipe's `swap_with_neighbour`, which
        // runs after `assign_missing_display_order` inside the same atomic update, so a
        // half-numbered stack never reaches it. The rendered list is ordered by the web's
        // `orderStackNodes`, which deliberately diverges here: it falls back to topological order
        // *wholesale* for this input (PRD D25), because interleaving real positions with invented
        // ones can put a child above its parent. Pinned so a change to the key is a decision rather
        // than an accident.
        let stack = a_stack(vec![
            a_node("n1", &[], None),
            a_node("n2", &[], Some(0)),
            a_node("n3", &[], None),
        ]);

        // When
        let order = stack.display_order();

        // Then
        assert_eq!(order, vec!["n2", "n1", "n3"]);
    }

    #[test]
    fn display_order_still_lists_every_node_when_the_dag_holds_a_cycle() {
        // Given — a cycle, which `topo_order` refuses outright. The two nodes are declared n2 before
        // n1, so declaration order and the sort key's final lexicographic tie-break disagree: a key
        // that had lost its declaration-order fallback would answer `n1, n2` instead.
        let stack = a_stack(vec![
            a_node("n2", &["n1"], None),
            a_node("n1", &["n2"], None),
        ]);

        // When — this is a render path: a list the operator cannot see is worse than one in an
        // imperfect order
        let order = stack.display_order();

        // Then — the tie-break degrades to where the nodes are declared, and nothing is dropped
        assert!(stack.topo_order().is_err());
        assert_eq!(order, vec!["n2", "n1"]);
    }

    #[test]
    fn update_stack_atomic_reads_and_writes_back() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![],
            }),
            ..Default::default()
        };
        write_changeset(dir, &cs).unwrap();

        update_stack_atomic(dir, |stack| {
            stack.version = 42;
        })
        .unwrap();

        let loaded = read_changeset(dir).unwrap();
        assert_eq!(loaded.stack.unwrap().version, 42);
    }

    #[test]
    fn link_stack_node_to_child_session_sets_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Default::default()
        };
        write_changeset(dir, &cs).unwrap();

        link_stack_node_to_child_session(dir, "n1", "sess-abc", Some("feature/n1".to_string()))
            .unwrap();

        let loaded = read_changeset(dir).unwrap();
        let node = loaded.stack.unwrap();
        let n1 = node.node("n1").unwrap();
        assert_eq!(n1.session_id.as_deref(), Some("sess-abc"));
        assert_eq!(n1.branch.as_deref(), Some("feature/n1"));
    }

    #[test]
    fn sync_stack_node_from_child_propagates_state_and_pr_status() {
        use tddy_workflow::ids::WorkflowState;
        let orch_tmp = tempfile::tempdir().unwrap();
        let sessions_tmp = tempfile::tempdir().unwrap();
        let orch_dir = orch_tmp.path();
        let sessions_root = sessions_tmp.path();
        let child_id = "child-session-1";

        // unified_session_dir_path = sessions_root/sessions/<id>
        let child_dir = sessions_root.join("sessions").join(child_id);
        std::fs::create_dir_all(&child_dir).unwrap();
        let mut child_cs = Changeset::default();
        child_cs.state.current = WorkflowState::new("GreenImplementing");
        child_cs.workflow = Some(ChangesetWorkflow {
            github_pr_status: Some(GithubPrStatus {
                phase: "open".to_string(),
                url: Some("https://github.com/example/pr/1".to_string()),
                error: None,
            }),
            ..Default::default()
        });
        write_changeset(&child_dir, &child_cs).unwrap();

        let orch_cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Node 1".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/n1".to_string()),
                    session_id: Some(child_id.to_string()),
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Default::default()
        };
        write_changeset(orch_dir, &orch_cs).unwrap();

        sync_stack_node_from_child(orch_dir, sessions_root, "n1").unwrap();

        let loaded = read_changeset(orch_dir).unwrap();
        let stack = loaded.stack.unwrap();
        let n1 = stack.node("n1").unwrap();
        assert_eq!(
            n1.child_state,
            Some(WorkflowState::new("GreenImplementing"))
        );
        assert_eq!(n1.pr_status.as_ref().unwrap().phase, "open");
    }

    #[test]
    fn serde_back_compat_changeset_without_stack_field() {
        let yaml = r#"
version: 1
models: {}
sessions: []
state:
  current: Init
  updated_at: "2024-01-01T00:00:00Z"
  history: []
artifacts: {}
discovery: ~
"#;
        let cs: Changeset =
            serde_yaml::from_str(yaml).expect("legacy changeset should deserialize");
        assert!(
            cs.stack.is_none(),
            "stack should be None for legacy changeset"
        );
        assert!(
            cs.orchestrator_session_id.is_none(),
            "orchestrator_session_id should be None for legacy changeset"
        );
    }
}
