//! Acceptance: a planned PR owns no branch until a child worktree creates one.
//!
//! The remote branch name is the durable link between a planned PR, its worktree/session, and its
//! GitHub PR — and the stack's spawn ordering is gated on it: a node's descendants base onto
//! `origin/<branch>`. So `branch` means "a branch that exists". At planning time only a
//! `branch_suggestion` is known, from the agent plan (`planned_prs_into_stack_nodes`) or from a
//! manual add (`add_planned_pr_node`); pre-filling `branch` from it would unblock descendants onto
//! a ref nothing created.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 1.

use tddy_core::changeset::Changeset;
use tddy_workflow_recipes::plan_pr_stack::{planned_prs_into_stack_nodes, PlannedPr};
use tddy_workflow_recipes::pr_stack::{add_planned_pr_node, AddPlannedPrInput};

#[test]
fn a_manually_added_planned_pr_keeps_its_branch_name_as_a_suggestion_only() {
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

    // Then — the proposed name is recorded as a suggestion, and no branch is claimed yet
    assert_eq!(
        node.branch_suggestion.as_deref(),
        Some("feature/auth/token-store")
    );
    assert_eq!(node.branch, None);
}

#[test]
fn a_planned_pr_from_the_agent_plan_keeps_its_branch_name_as_a_suggestion_only() {
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

    // Then — the proposed name is recorded as a suggestion, and no branch is claimed yet
    assert_eq!(
        nodes[0].branch_suggestion.as_deref(),
        Some("feature/auth/token-store")
    );
    assert_eq!(nodes[0].branch, None);
}
