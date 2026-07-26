//! Acceptance: a planned node's spawn base is its parent's branch, not the stack default.
//!
//! `Stack::base_ref_for_spawn` is the single source of truth both daemon spawn paths (the web
//! "Start session" button and the orchestrator agent's `spawn-child`) consult when creating a
//! child worktree. It returns the effective base ref for a node — the nearest non-merged
//! ancestor's `origin/<branch>`.
//!
//! What gates a spawn is the parent's **branch**: that is the ref the child worktree branches off,
//! so a parent that owns no branch yet has nothing to offer and the spawn is refused. Child
//! sessions are deliberately not part of this decision — a node whose branch exists can be built
//! on whether or not a session is (still) attached to it. `branch_suggestion` is a planned name,
//! not a ref, and never satisfies the gate.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § capability 5.

use tddy_core::changeset::{GithubPrStatus, Stack, StackNode};
use tddy_core::WorkflowError;

// --- builders ---------------------------------------------------------------

/// A planned node: named and ordered, but nothing of it exists in git yet.
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
    }
}

/// A node that owns a real branch — the only state that lets descendants base onto it.
fn a_node_on_branch(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        branch: Some(branch.to_string()),
        ..a_planned_node(node_id)
    }
}

/// A branch-owning node whose PR has already merged (skipped for base purposes).
fn a_merged_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        pr_status: Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        }),
        ..a_node_on_branch(node_id, branch)
    }
}

fn a_child_of(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..a_planned_node(node_id)
    }
}

fn a_stack(nodes: Vec<StackNode>) -> Stack {
    Stack { version: 1, nodes }
}

// --- assertions -------------------------------------------------------------

struct SpawnRefusal(WorkflowError);

fn assert_spawn_refusal(result: Result<String, WorkflowError>) -> SpawnRefusal {
    match result {
        Err(e) => SpawnRefusal(e),
        Ok(base) => panic!("expected a refusal to resolve a spawn base, but it resolved to {base}"),
    }
}

impl SpawnRefusal {
    fn names_the_blocking_node(self, node_id: &str) -> Self {
        let msg = self.0.to_string();
        assert!(
            msg.contains(node_id),
            "refusal should name the blocking node '{node_id}', got: {msg}"
        );
        self
    }

    fn explains_the_missing_branch(self) -> Self {
        let msg = self.0.to_string();
        assert!(
            msg.contains("no branch"),
            "refusal should explain that the parent owns no branch, got: {msg}"
        );
        self
    }
}

// --- tests ------------------------------------------------------------------

#[test]
fn bases_a_node_on_its_non_merged_parent_branch() {
    // Given — n1 owns feature/x/n1 with no session attached; n2 depends on n1
    let stack = a_stack(vec![
        a_node_on_branch("n1", "feature/x/n1"),
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let base = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — an existing branch is all a descendant needs
    assert_eq!(base.unwrap(), "origin/feature/x/n1");
}

#[test]
fn bases_a_node_on_its_parent_branch_when_the_parent_still_has_a_child_session() {
    // Given — the same stack, except n1's child session is recorded alongside its branch
    let stack = a_stack(vec![
        StackNode {
            session_id: Some("session-for-n1".to_string()),
            ..a_node_on_branch("n1", "feature/x/n1")
        },
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let base = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — the session neither adds to nor subtracts from the branch decision
    assert_eq!(base.unwrap(), "origin/feature/x/n1");
}

#[test]
fn bases_a_root_node_on_the_stack_default_branch() {
    // Given — n1 has no parents
    let stack = a_stack(vec![a_planned_node("n1")]);

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
fn refuses_to_resolve_a_base_while_a_non_merged_parent_owns_no_branch() {
    // Given — n1 is still purely planned; n2 depends on n1
    let stack = a_stack(vec![a_planned_node("n1"), a_child_of("n2", &["n1"])]);

    // When
    let refusal = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — there is no ref to branch off, and the refusal says so
    assert_spawn_refusal(refusal)
        .names_the_blocking_node("n1")
        .explains_the_missing_branch();
}

#[test]
fn refuses_to_resolve_a_base_from_a_parent_that_only_has_a_suggested_branch_name() {
    // Given — n1 carries the branch name the planner proposed, but nothing created it yet
    let stack = a_stack(vec![
        StackNode {
            branch_suggestion: Some("feature/x/n1".to_string()),
            ..a_planned_node("n1")
        },
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let refusal = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — a suggestion is a plan, not a ref
    assert_spawn_refusal(refusal)
        .names_the_blocking_node("n1")
        .explains_the_missing_branch();
}

#[test]
fn refuses_to_invent_a_base_ref_from_a_parent_node_id() {
    // Given — n1 has a child session but never recorded the branch that session created
    let stack = a_stack(vec![
        StackNode {
            session_id: Some("session-for-n1".to_string()),
            ..a_planned_node("n1")
        },
        a_child_of("n2", &["n1"]),
    ]);

    // When
    let refusal = stack.base_ref_for_spawn("n2", "origin/master");

    // Then — refused rather than resolved to `origin/n1`, a ref that does not exist
    assert_spawn_refusal(refusal)
        .names_the_blocking_node("n1")
        .explains_the_missing_branch();
}
