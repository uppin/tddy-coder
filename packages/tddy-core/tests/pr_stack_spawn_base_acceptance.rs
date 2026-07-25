//! Acceptance: a planned node's spawn base is its parent's branch, not the stack default.
//!
//! `Stack::base_ref_for_spawn` is the single source of truth both daemon spawn paths (the web
//! "Start session" button and the orchestrator agent's `spawn-child`) consult when creating a
//! child worktree. It returns the effective base ref for a node — the nearest non-merged
//! ancestor's `origin/<branch>` — and refuses to resolve while a non-merged parent is un-started,
//! so the stack sequence is respected and children stack bottom-up.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 5.

use tddy_core::changeset::{GithubPrStatus, Stack, StackNode};

// --- builders ---------------------------------------------------------------

fn a_node(node_id: &str) -> StackNode {
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
    }
}

/// A node that has been started: it owns a branch and a live child session.
fn a_started_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        branch: Some(branch.to_string()),
        session_id: Some(format!("session-for-{node_id}")),
        ..a_node(node_id)
    }
}

/// A started node whose PR has already merged (skipped for base purposes).
fn a_merged_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        pr_status: Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        }),
        ..a_started_node(node_id, branch)
    }
}

fn a_child_of(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..a_node(node_id)
    }
}

fn a_stack(nodes: Vec<StackNode>) -> Stack {
    Stack { version: 1, nodes }
}

// --- tests ------------------------------------------------------------------

#[test]
fn bases_a_node_on_its_non_merged_parent_branch() {
    // Given — n1 is started on feature/x/n1; n2 depends on n1
    let stack = a_stack(vec![
        a_started_node("n1", "feature/x/n1"),
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let base = stack.base_ref_for_spawn("n2", "origin/master");

    // Then
    assert_eq!(base.unwrap(), "origin/feature/x/n1");
}

#[test]
fn bases_a_root_node_on_the_stack_default_branch() {
    // Given — n1 has no parents
    let stack = a_stack(vec![a_node("n1")]);

    // When
    let base = stack.base_ref_for_spawn("n1", "origin/master");

    // Then
    assert_eq!(base.unwrap(), "origin/master");
}

#[test]
fn bases_a_node_on_the_stack_default_when_its_only_parent_is_merged() {
    // Given — n1 has merged; n2's only parent is n1
    let stack = a_stack(vec![
        a_merged_node("n1", "feature/x/n1"),
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let base = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — the merged ancestor is skipped, so n2 bases off the stack default
    assert_eq!(base.unwrap(), "origin/master");
}

#[test]
fn refuses_to_resolve_a_base_when_a_non_merged_parent_is_unstarted() {
    // Given — n1 is planned but not started (no session, still open); n2 depends on n1
    let stack = a_stack(vec![
        StackNode {
            branch: Some("feature/x/n1".to_string()),
            session_id: None,
            ..a_node("n1")
        },
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let err = stack
        .base_ref_for_spawn("n2", "origin/master")
        .expect_err("expected a refusal while parent n1 is un-started");

    // Then — the error names the un-started parent
    assert!(
        err.to_string().contains("n1"),
        "error should name the un-started parent n1, got: {err}"
    );
}
