//! Acceptance: a planned PR is assigned a definitive branch at creation.
//!
//! The remote branch name is the durable link between a planned PR, its worktree/session, and its
//! GitHub PR. It must be set on the node when the node is created — from the agent plan
//! (`planned_prs_into_stack_nodes`) or from a manual add (`add_planned_pr_node`) — not deferred
//! until a worktree exists.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 1.

use tddy_core::changeset::Changeset;
use tddy_workflow_recipes::plan_pr_stack::{planned_prs_into_stack_nodes, PlannedPr};
use tddy_workflow_recipes::pr_stack::{add_planned_pr_node, AddPlannedPrInput};

#[test]
fn add_planned_pr_node_assigns_a_definitive_branch_at_creation() {
    // Given — a fresh orchestrator session with no stack yet
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();

    // When — a planned PR is added with a branch suggestion
    let node = add_planned_pr_node(
        dir,
        AddPlannedPrInput {
            title: "Add token store".to_string(),
            description: String::new(),
            branch_suggestion: Some("feature/auth/token-store".to_string()),
            parents: vec![],
            child_recipe: None,
        },
    )
    .expect("add_planned_pr_node should succeed");

    // Then — the node carries a definitive branch, ready to link a worktree/session/PR
    assert_eq!(node.branch.as_deref(), Some("feature/auth/token-store"));
}

#[test]
fn planned_prs_into_stack_nodes_assigns_branch_from_the_branch_suggestion() {
    // Given — an agent plan entry with a branch suggestion
    let pr = PlannedPr {
        node_id: "n1".to_string(),
        title: "Add token store".to_string(),
        description: String::new(),
        branch_suggestion: Some("feature/auth/token-store".to_string()),
        parents: vec![],
        child_recipe: None,
    };

    // When
    let nodes = planned_prs_into_stack_nodes(&[pr]);

    // Then — the converted node's branch is the definitive link key
    assert_eq!(nodes[0].branch.as_deref(), Some("feature/auth/token-store"));
}
