//! Assess task: reads the stack DAG + child session states + GitHub PR states,
//! then decides the next orchestrator action.

use std::path::Path;

use async_trait::async_trait;
use tddy_core::changeset::Stack;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::{NextAction, Task, TaskResult};

use super::github::GithubPrApi;

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

/// Effective base ref for a node (skips merged ancestors; returns default_branch when all merged).
pub fn effective_base_ref(node_id: &str, views: &[NodeView], default_branch: &str) -> String {
    let Some(node) = views.iter().find(|v| v.node_id == node_id) else {
        return default_branch.to_string();
    };
    if node.parent_dep_ids.is_empty() {
        return default_branch.to_string();
    }
    // Walk each parent; if all are merged, collapse to default_branch.
    // Otherwise return the branch of the nearest unmerged parent.
    for parent_id in &node.parent_dep_ids {
        let Some(parent_view) = views.iter().find(|v| &v.node_id == parent_id) else {
            continue;
        };
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
    stack: &Stack,
    gh: &dyn GithubPrApi,
    _default_branch: &str,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    use tddy_core::changeset::read_changeset;
    use tddy_core::session_lifecycle::unified_session_dir_path;

    let order = stack.topo_order()?;

    let mut views: Vec<NodeView> = Vec::with_capacity(stack.nodes.len());

    for node_id in &order {
        let node = stack
            .nodes
            .iter()
            .find(|n| &n.node_id == node_id)
            .expect("topo_order only contains node ids from the stack");

        let branch = node
            .branch
            .clone()
            .unwrap_or_else(|| format!("feature/{node_id}"));

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

        let pr = if node.session_id.is_some() {
            match gh.get_open_pr(&branch)? {
                Some(pr_ref) => PrLiveStatus::Open {
                    number: pr_ref.number,
                    base: pr_ref.base_branch,
                },
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

/// The assess Task: reads stack, calls assemble_views + decide_next_action, returns GoTo.
pub struct AssessTask {}

impl AssessTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for AssessTask {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Task for AssessTask {
    fn id(&self) -> &str {
        "assess"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        use tddy_core::changeset::read_changeset;

        let session_dir = context
            .get_sync::<std::path::PathBuf>("session_dir")
            .ok_or("assess: session_dir not set in context")?;
        // sessions_root = sessions_base (two levels above session_dir: .../sessions/{id}/ → ...)
        let sessions_root = session_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| session_dir.clone());

        let changeset = read_changeset(&session_dir)?;
        let stack = changeset.stack.clone().unwrap_or_default();
        let default_branch = context
            .get_sync::<String>("default_branch")
            .unwrap_or_else(|| "master".to_string());
        let repo_root = changeset
            .repo_path
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| session_dir.clone());
        // Derive "owner/repo" from the git remote URL so GitHub API calls use the
        // correct namespace (changeset.repo_path is a filesystem path, not owner/repo).
        let github_owner_repo = {
            let remote_url = std::process::Command::new("git")
                .current_dir(&repo_root)
                .args(["remote", "get-url", "origin"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            super::github::owner_repo_from_remote_url(&remote_url)
                .unwrap_or_else(|| context.get_sync::<String>("repo").unwrap_or_default())
        };

        let gh = super::github::RealGithubPrApi::new(&github_owner_repo);

        let views = assemble_views(&session_dir, &sessions_root, &stack, &gh, &default_branch)?;
        let autonomous_merge = context
            .get_sync::<bool>("autonomous_merge")
            .unwrap_or(false);
        let approved: std::collections::HashSet<String> = context
            .get_sync::<Vec<String>>("approved_nodes")
            .unwrap_or_default()
            .into_iter()
            .collect();

        let action = decide_next_action(&views, autonomous_merge, &approved);

        let next_state = match &action {
            OrchestratorAction::Spawn { node_ids } => {
                context.set_sync("spawn_node_ids", node_ids.clone());
                "spawn"
            }
            OrchestratorAction::Merge { node_id, pr_number } => {
                context.set_sync("merge_node_id", node_id.clone());
                context.set_sync("merge_pr_number", *pr_number);
                "merge"
            }
            OrchestratorAction::Wait { reason } => {
                context.set_sync("wait_reason", reason.clone());
                "wait"
            }
            OrchestratorAction::MarkFailed { node_id, reason } => {
                context.set_sync("failed_node_id", node_id.clone());
                context.set_sync("failed_reason", reason.clone());
                "failed"
            }
            OrchestratorAction::Done => "done",
        };

        Ok(TaskResult {
            response: format!("assess → {next_state}"),
            next_action: NextAction::GoTo(next_state.to_string()),
            task_id: self.id().to_string(),
            status_message: None,
        })
    }
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
        let views = vec![node_view(
            "n1",
            &[],
            ChildPhase::NotSpawned,
            PrLiveStatus::None,
        )];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        assert!(
            matches!(&action, OrchestratorAction::Spawn { node_ids } if node_ids == &["n1".to_string()]),
            "expected Spawn for unspawned root, got: {action:?}"
        );
    }

    /// A node with a spawn already in flight (marker file written, daemon hasn't yet created the
    /// child session and set `session_id`) must not be re-spawned on the next assess tick — that
    /// produces an unbounded assess→spawn→assess busy-loop (reproduced live: 1,634+ repeated
    /// spawn-request writes for the same node in under a second, because the same `NotSpawned`
    /// view kept winning `decide_next_action`'s spawn rule before the async spawn ever completed).
    #[test]
    fn decide_next_action_waits_when_a_node_has_a_pending_spawn_request() {
        let views = vec![node_view(
            "n1",
            &[],
            ChildPhase::SpawnRequested,
            PrLiveStatus::None,
        )];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        assert!(
            matches!(action, OrchestratorAction::Wait { .. }),
            "expected Wait for a node with a spawn request already in flight, got: {action:?}"
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
            PrLiveStatus::Open {
                number: 7,
                base: "master".to_string(),
            },
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
        assert_eq!(
            action,
            OrchestratorAction::Done,
            "expected Done when all merged"
        );
    }

    // -----------------------------------------------------------------------
    // effective_base_ref unit tests
    // -----------------------------------------------------------------------

    fn node_view_with_pr(
        node_id: &str,
        parents: &[&str],
        phase: ChildPhase,
        pr: PrLiveStatus,
    ) -> NodeView {
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

    /// `effective_base_ref_skips_merged_ancestor_returns_default_branch` — when the only
    /// parent node's PR is merged, `effective_base_ref` must skip it and return `default_branch`.
    #[test]
    fn effective_base_ref_skips_merged_ancestor_returns_default_branch() {
        // n1 (root) → merged PR
        // n2 depends on n1 (which is merged) → effective base should collapse to default_branch
        let views = vec![
            node_view_with_pr("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged),
            node_view_with_pr(
                "n2",
                &["n1"],
                ChildPhase::PrOpen,
                PrLiveStatus::Open {
                    number: 2,
                    base: "feature/n1".into(),
                },
            ),
        ];

        let base = effective_base_ref("n2", &views, "master");

        assert_eq!(
            base, "master",
            "effective base of n2 must be 'master' when its only parent (n1) is already merged"
        );
    }

    /// `effective_base_ref_returns_nearest_unmerged_ancestor_branch` — when a middle ancestor is
    /// not yet merged, `effective_base_ref` must return that ancestor's branch.
    #[test]
    fn effective_base_ref_returns_nearest_unmerged_ancestor_branch() {
        // Linear: n1 (merged) → n2 (open PR, base=master after n1 merge) → n3 (open, base=feature/n2)
        // n2 is still open, so n3's effective base is feature/n2
        let views = vec![
            node_view_with_pr("n1", &[], ChildPhase::PrOpen, PrLiveStatus::Merged),
            node_view_with_pr(
                "n2",
                &["n1"],
                ChildPhase::PrOpen,
                PrLiveStatus::Open {
                    number: 2,
                    base: "master".into(),
                },
            ),
            node_view_with_pr(
                "n3",
                &["n2"],
                ChildPhase::PrOpen,
                PrLiveStatus::Open {
                    number: 3,
                    base: "feature/n2".into(),
                },
            ),
        ];

        let base = effective_base_ref("n3", &views, "master");

        assert_eq!(
            base, "feature/n2",
            "effective base of n3 must be feature/n2 (its unmerged parent branch), not 'master'"
        );
    }

    /// `effective_base_ref_root_node_returns_default_branch` — a root node with no parents
    /// must always return `default_branch` as its effective base.
    #[test]
    fn effective_base_ref_root_node_returns_default_branch() {
        let views = vec![node_view_with_pr(
            "n1",
            &[],
            ChildPhase::NotSpawned,
            PrLiveStatus::None,
        )];

        let base = effective_base_ref("n1", &views, "master");

        assert_eq!(
            base, "master",
            "root node with no parents must use default_branch as effective base"
        );
    }

    // -----------------------------------------------------------------------
    // assemble_views unit tests (via MockGithubPrApi)
    // -----------------------------------------------------------------------

    struct AlwaysOpenMockGh {
        pr_number: u64,
        base: String,
    }
    impl crate::orchestrate_pr_stack::github::GithubPrApi for AlwaysOpenMockGh {
        fn get_open_pr(
            &self,
            _head: &str,
        ) -> Result<Option<crate::orchestrate_pr_stack::github::PrRef>, tddy_core::WorkflowError>
        {
            Ok(Some(crate::orchestrate_pr_stack::github::PrRef {
                number: self.pr_number,
                head_sha: "sha".into(),
                base_branch: self.base.clone(),
                url: "https://github.com/o/r/pull/1".into(),
            }))
        }
        fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
            Ok("sha".into())
        }
        fn patch_pr_base(
            &self,
            _number: u64,
            _new_base: &str,
        ) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
        fn create_pr(
            &self,
            _head: &str,
            _base: &str,
            _title: &str,
            _body: &str,
        ) -> Result<u64, tddy_core::WorkflowError> {
            Ok(99)
        }
        fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
    }

    struct NoneOpenMockGh;
    impl crate::orchestrate_pr_stack::github::GithubPrApi for NoneOpenMockGh {
        fn get_open_pr(
            &self,
            _head: &str,
        ) -> Result<Option<crate::orchestrate_pr_stack::github::PrRef>, tddy_core::WorkflowError>
        {
            Ok(None)
        }
        fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
            Ok("sha".into())
        }
        fn patch_pr_base(
            &self,
            _number: u64,
            _new_base: &str,
        ) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
        fn create_pr(
            &self,
            _head: &str,
            _base: &str,
            _title: &str,
            _body: &str,
        ) -> Result<u64, tddy_core::WorkflowError> {
            Ok(99)
        }
        fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
    }

    /// `assemble_views_marks_notspawned_when_session_id_absent` — when a `StackNode` has
    /// `session_id == None`, `assemble_views` must produce a `NodeView` with
    /// `child_phase == NotSpawned`, and the views must be in topological order.
    #[test]
    fn assemble_views_marks_notspawned_when_session_id_absent() {
        use tddy_core::changeset::{Stack, StackNode};

        let tmp = tempfile::tempdir().unwrap();
        let parent_dir = tmp.path();
        let sessions_root = tmp.path();

        // Write an orchestrator changeset with two nodes: n1 (root), n2 (depends on n1)
        let stack = Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".into(),
                    title: "Root".into(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None, // not spawned
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".into(),
                    title: "Dep".into(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None, // not spawned
                    parents: vec!["n1".into()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        };
        let mock_gh = NoneOpenMockGh;

        let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
            .expect("assemble_views must not error for unspawned nodes");

        assert_eq!(views.len(), 2, "must produce one view per node");

        // Topological order: n1 before n2
        assert_eq!(
            views[0].node_id, "n1",
            "n1 (root) must appear before n2 in topo order"
        );
        assert_eq!(
            views[1].node_id, "n2",
            "n2 (dependent) must appear after n1"
        );

        assert!(
            matches!(views[0].child_phase, ChildPhase::NotSpawned),
            "n1 must be NotSpawned (no session_id); got: {:?}",
            views[0].child_phase
        );
        assert!(
            matches!(views[1].child_phase, ChildPhase::NotSpawned),
            "n2 must be NotSpawned (no session_id); got: {:?}",
            views[1].child_phase
        );
    }

    /// `assemble_views_marks_spawn_requested_when_a_pending_spawn_request_marker_file_exists` —
    /// `SpawnTask` writes `.workflow/spawn-request-<node_id>.json` before the daemon has had a
    /// chance to actually create the child session and set `session_id`. While that marker is
    /// present, `assemble_views` must report `SpawnRequested` (not `NotSpawned`), so
    /// `decide_next_action` doesn't re-issue the same spawn on the very next assess tick.
    #[test]
    fn assemble_views_marks_spawn_requested_when_a_pending_spawn_request_marker_file_exists() {
        use tddy_core::changeset::{Stack, StackNode};

        let tmp = tempfile::tempdir().unwrap();
        let parent_dir = tmp.path();
        let sessions_root = tmp.path();

        let workflow_dir = parent_dir.join(".workflow");
        std::fs::create_dir_all(&workflow_dir).unwrap();
        std::fs::write(
            workflow_dir.join("spawn-request-n1.json"),
            r#"{"node_id":"n1"}"#,
        )
        .unwrap();

        let stack = Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Root".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None, // daemon hasn't created the child session yet
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        };
        let mock_gh = NoneOpenMockGh;

        let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
            .expect("assemble_views must not error");

        assert_eq!(views.len(), 1);
        assert!(
            matches!(views[0].child_phase, ChildPhase::SpawnRequested),
            "n1 must be SpawnRequested while its spawn-request marker file exists; got: {:?}",
            views[0].child_phase
        );
    }

    /// `assemble_views_reports_propen_when_mock_returns_open_pr` — when the `GithubPrApi` mock
    /// returns an open PR for a node's branch, `assemble_views` must map it to `PrLiveStatus::Open`.
    #[test]
    fn assemble_views_reports_propen_when_mock_returns_open_pr() {
        use tddy_core::changeset::{write_changeset_atomic, Changeset, Stack, StackNode};
        use tddy_core::session_lifecycle::unified_session_dir_path;
        use tddy_core::WorkflowState;

        let tmp = tempfile::tempdir().unwrap();
        let parent_dir = tmp.path();
        let sessions_root = tmp.path();

        // Child session dir for n1
        let child_dir = unified_session_dir_path(sessions_root, "child-sess-n1");
        std::fs::create_dir_all(&child_dir).unwrap();
        let mut child_cs = Changeset::default();
        child_cs.state.current = WorkflowState::new("Done");
        write_changeset_atomic(&child_dir, &child_cs).unwrap();

        let stack = Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Root".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: Some("feature/n1".into()),
                session_id: Some("child-sess-n1".into()),
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        };

        let mock_gh = AlwaysOpenMockGh {
            pr_number: 7,
            base: "master".into(),
        };

        let views = assemble_views(parent_dir, sessions_root, &stack, &mock_gh, "master")
            .expect("assemble_views must not error");

        assert_eq!(views.len(), 1);
        assert!(
            matches!(&views[0].pr, PrLiveStatus::Open { number: 7, .. }),
            "PR status must be Open {{ number: 7 }} when mock returns an open PR; got: {:?}",
            views[0].pr
        );
    }

    #[test]
    fn decide_next_action_wait_for_merge_gate_when_autonomous_merge_off() {
        // One node: deps merged, PR open and base is already master — normally ready to merge.
        // With autonomous_merge = false (the default), decide_next_action should return Wait.
        let views = vec![node_view(
            "n1",
            &[],
            ChildPhase::PrOpen,
            PrLiveStatus::Open {
                number: 3,
                base: "master".to_string(),
            },
        )];
        let action = decide_next_action(&views, false, &std::collections::HashSet::new());
        // Gate-off default: expect Wait, not Merge
        assert!(
            matches!(action, OrchestratorAction::Wait { .. }),
            "expected Wait when merge gate is off (operator-gated default), got: {action:?}"
        );
    }
}
