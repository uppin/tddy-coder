//! Acceptance: a planned PR owns no branch until a child worktree creates one.
//!
//! The remote branch name is the durable link between a planned PR, its worktree/session, and its
//! GitHub PR — and the stack's spawn ordering is gated on it: a node's descendants base onto
//! `origin/<branch>`. So `branch` means "a branch that exists". At planning time only a
//! `branch_suggestion` is known, from a manual add (`add_work_item`); pre-filling `branch` from
//! it would unblock descendants onto a ref nothing created.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 1.

use tddy_core::changeset::{Changeset, WorkQueue};
use tddy_work_queue::item_ops::{add_work_item, AddWorkItemInput};

#[test]
fn a_manually_added_planned_pr_keeps_its_branch_name_as_a_suggestion_only() {
    // Given — a fresh orchestrator session whose work queue, created at its start, is still empty
    let tmp = tddy_changeset::work_queue_fixtures::a_stack_session_dir();
    let dir = tmp.path();
    tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();
    tddy_changeset::work_queue_fixtures::seed_session_stack(dir, WorkQueue::default());

    // When — a planned PR is added with a branch suggestion
    let node = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add token store".to_string(),
            description: String::new(),
            branch_suggestion: Some("feature/auth/token-store".to_string()),
            parents: vec![],
            child_recipe: None,
        },
    )
    .expect("add_work_item should succeed");

    // Then — the proposed name is recorded as a suggestion, and no branch is claimed yet
    assert_eq!(
        node.branch_suggestion.as_deref(),
        Some("feature/auth/token-store")
    );
    assert_eq!(node.branch, None);
}
