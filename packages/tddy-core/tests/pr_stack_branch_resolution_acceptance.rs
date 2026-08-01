//! Acceptance: a stack node's branch is resolved from the node, with its child session as fallback.
//!
//! The stack progresses on branches. A node records the branch its child worktree created, and that
//! recorded value is authoritative. A child session is only a *fallback* route to the same answer:
//! when a node has a session but no branch of its own (an older manifest, or a link that landed
//! before the branch was known), the branch is read out of that child session's changeset.
//!
//! Sessions are otherwise irrelevant to stack progression — a node whose branch is known needs no
//! session, and a session that has been deleted does not un-resolve a branch already recorded.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 5.

use std::path::Path;
use tddy_core::changeset::{
    read_stack_with_resolved_branches, resolve_stack_node_branch, Changeset, Stack, StackNode,
};
use tddy_core::session_lifecycle::unified_session_dir_path;

// --- builders ---------------------------------------------------------------

fn a_planned_node(node_id: &str) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: None,
        branch: None,
        session_id: None,
        parents: Vec::new(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

fn a_node_on_branch(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        branch: Some(branch.to_string()),
        ..a_planned_node(node_id)
    }
}

fn a_node_linked_to_session(node_id: &str, session_id: &str) -> StackNode {
    StackNode {
        session_id: Some(session_id.to_string()),
        ..a_planned_node(node_id)
    }
}

fn a_child_of(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..a_planned_node(node_id)
    }
}

/// A materialized child session sitting on `branch`.
fn a_child_session_on_branch(sessions_root: &Path, session_id: &str, branch: &str) {
    a_child_session(
        sessions_root,
        session_id,
        Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    );
}

/// A child session that exists but has not created its branch yet.
fn a_branchless_child_session(sessions_root: &Path, session_id: &str) {
    a_child_session(
        sessions_root,
        session_id,
        Changeset {
            branch: None,
            ..Changeset::default()
        },
    );
}

fn a_child_session(sessions_root: &Path, session_id: &str, changeset: Changeset) {
    let dir = unified_session_dir_path(sessions_root, session_id);
    std::fs::create_dir_all(&dir).expect("create child session dir");
    tddy_core::changeset::write_changeset(&dir, &changeset).expect("write child changeset");
}

/// A pr-stack orchestrator session owning `stack`.
fn an_orchestrator_with_stack(sessions_root: &Path, session_id: &str, nodes: Vec<StackNode>) {
    let dir = unified_session_dir_path(sessions_root, session_id);
    std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
    let changeset = Changeset {
        recipe: Some("pr-stack".to_string()),
        stack: Some(Stack { version: 1, nodes }),
        branch: None,
        ..Changeset::default()
    };
    tddy_core::changeset::write_changeset(&dir, &changeset).expect("write orchestrator changeset");
}

fn a_sessions_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp sessions root")
}

// --- tests ------------------------------------------------------------------

#[test]
fn resolves_the_branch_a_node_recorded_itself() {
    // Given — n1 recorded its branch and has no session attached
    let root = a_sessions_root();
    let node = a_node_on_branch("n1", "feature/x/n1");

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then
    assert_eq!(branch, Some("feature/x/n1".to_string()));
}

#[test]
fn resolves_the_branch_from_the_child_session_when_the_node_recorded_none() {
    // Given — n1 knows only its child session; the branch lives in that session's changeset
    let root = a_sessions_root();
    a_child_session_on_branch(root.path(), "child-1", "feature/x/n1");
    let node = a_node_linked_to_session("n1", "child-1");

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then — the session is the fallback route to the branch
    assert_eq!(branch, Some("feature/x/n1".to_string()));
}

#[test]
fn prefers_the_branch_the_node_recorded_over_its_child_session_branch() {
    // Given — the node and its child session disagree about the branch
    let root = a_sessions_root();
    a_child_session_on_branch(root.path(), "child-1", "feature/x/from-session");
    let node = StackNode {
        session_id: Some("child-1".to_string()),
        ..a_node_on_branch("n1", "feature/x/from-node")
    };

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then — the node's own record wins; the session is only a fallback
    assert_eq!(branch, Some("feature/x/from-node".to_string()));
}

#[test]
fn resolves_no_branch_when_neither_the_node_nor_its_child_session_names_one() {
    // Given — the child session exists but has not created a branch yet
    let root = a_sessions_root();
    a_branchless_child_session(root.path(), "child-1");
    let node = a_node_linked_to_session("n1", "child-1");

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then
    assert_eq!(branch, None);
}

#[test]
fn resolves_no_branch_when_the_linked_child_session_is_gone_from_disk() {
    // Given — n1 points at a session directory that no longer exists
    let root = a_sessions_root();
    let node = a_node_linked_to_session("n1", "child-deleted");

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then — a missing session resolves to no branch rather than failing
    assert_eq!(branch, None);
}

#[test]
fn resolves_no_branch_for_a_node_that_has_neither_a_branch_nor_a_session() {
    // Given — a purely planned node
    let root = a_sessions_root();
    let node = a_planned_node("n1");

    // When
    let branch = resolve_stack_node_branch(root.path(), &node);

    // Then
    assert_eq!(branch, None);
}

#[test]
fn reads_the_orchestrator_stack_with_child_session_branches_filled_in() {
    // Given — an orchestrator whose n1 records only a child session, which owns the branch
    let root = a_sessions_root();
    a_child_session_on_branch(root.path(), "child-1", "feature/x/n1");
    an_orchestrator_with_stack(
        root.path(),
        "orchestrator-1",
        vec![a_node_linked_to_session("n1", "child-1")],
    );

    // When
    let stack = read_stack_with_resolved_branches(root.path(), "orchestrator-1")
        .expect("read orchestrator stack")
        .expect("orchestrator owns a stack");

    // Then — the hydrated node carries the branch its session created
    assert_eq!(
        stack.node("n1").expect("n1 in stack").branch,
        Some("feature/x/n1".to_string())
    );
}

#[test]
fn unblocks_a_dependent_node_once_its_parent_branch_resolves_through_the_child_session() {
    // Given — n1 records only a child session; n2 depends on n1
    let root = a_sessions_root();
    a_child_session_on_branch(root.path(), "child-1", "feature/x/n1");
    an_orchestrator_with_stack(
        root.path(),
        "orchestrator-1",
        vec![
            a_node_linked_to_session("n1", "child-1"),
            a_child_of("n2", &["n1"]),
        ],
    );

    // When — the hydrated stack resolves n2's spawn base
    let stack = read_stack_with_resolved_branches(root.path(), "orchestrator-1")
        .expect("read orchestrator stack")
        .expect("orchestrator owns a stack");
    let base = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — the fallback-resolved branch is a legitimate base for n2
    assert_eq!(base.unwrap(), "origin/feature/x/n1");
}

#[test]
fn reads_no_stack_from_a_session_that_owns_none() {
    // Given — an ordinary (non-orchestrator) session
    let root = a_sessions_root();
    a_child_session_on_branch(root.path(), "child-1", "feature/x/n1");

    // When
    let stack = read_stack_with_resolved_branches(root.path(), "child-1")
        .expect("read session without a stack");

    // Then
    assert_eq!(stack, None);
}
