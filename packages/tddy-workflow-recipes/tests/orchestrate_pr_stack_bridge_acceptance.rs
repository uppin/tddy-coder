//! PRD acceptance: orchestrate-pr-stack bridge — seed_orchestrator_stack_from_plan reads a
//! completed plan-pr-stack output and populates the orchestrator's Changeset.stack so the
//! first assess tick has a real Stack to operate on.

use std::fs;

use tddy_core::{read_changeset, write_changeset_atomic, Changeset};
use tddy_workflow_recipes::orchestrate_pr_stack::seed_orchestrator_stack_from_plan;
use tddy_workflow_recipes::plan_pr_stack::{StackPlanOutput, PlannedPr};

/// Helper: create a minimal changeset.yaml in a temp session dir.
fn write_empty_orchestrator_changeset(session_dir: &std::path::Path) {
    let cs = Changeset::default();
    write_changeset_atomic(session_dir, &cs).expect("write_changeset_atomic must succeed");
}

#[test]
fn seed_orchestrator_stack_from_plan_populates_changeset_stack() {
    // Given — an orchestrator session dir with an empty changeset (no stack yet)
    let tmp = tempfile::tempdir().expect("temp dir");
    let session_dir = tmp.path();
    write_empty_orchestrator_changeset(session_dir);

    // …and a 2-PR linear plan (n1 → n2)
    let plan = StackPlanOutput {
        version: 1,
        prs: vec![
            PlannedPr {
                node_id: "n1".into(),
                title: "Auth store".into(),
                description: "JWT token storage".into(),
                branch_suggestion: Some("feature/auth-store".into()),
                parents: vec![],
                child_recipe: Some("tdd".into()),
            },
            PlannedPr {
                node_id: "n2".into(),
                title: "Login API".into(),
                description: "POST /login".into(),
                branch_suggestion: Some("feature/login-api".into()),
                parents: vec!["n1".into()],
                child_recipe: None,
            },
        ],
    };

    // When — seeding the orchestrator's stack from the plan
    seed_orchestrator_stack_from_plan(session_dir, &plan)
        .expect("seed must succeed for a valid plan");

    // Then — the orchestrator's changeset now carries a Stack with 2 nodes
    let cs = read_changeset(session_dir).expect("changeset must be readable");
    let stack = cs.stack.expect("Changeset.stack must be Some after seeding");
    assert_eq!(stack.nodes.len(), 2, "must have 2 nodes matching the plan");
    assert_eq!(stack.nodes[0].node_id, "n1");
    assert_eq!(stack.nodes[1].node_id, "n2");

    // …and all session_ids are None (not yet materialised)
    assert!(
        stack.nodes.iter().all(|n| n.session_id.is_none()),
        "no child sessions yet — all session_ids must be None before orchestration starts"
    );

    // …and the parent relationship from the plan is preserved
    assert_eq!(
        stack.nodes[1].parents,
        vec!["n1".to_string()],
        "n2 must still list n1 as its parent"
    );
}

#[test]
fn seed_orchestrator_stack_from_plan_is_idempotent_on_empty_plan() {
    // Given — an orchestrator session dir and an empty plan (no PRs)
    let tmp = tempfile::tempdir().expect("temp dir");
    let session_dir = tmp.path();
    write_empty_orchestrator_changeset(session_dir);

    let plan = StackPlanOutput {
        version: 1,
        prs: vec![],
    };

    // When — seeding with an empty plan
    seed_orchestrator_stack_from_plan(session_dir, &plan)
        .expect("seeding an empty plan must not error");

    // Then — the changeset may have a stack but it must be empty-or-None; either is OK
    let cs = read_changeset(session_dir).expect("changeset readable");
    let node_count = cs.stack.map(|s| s.nodes.len()).unwrap_or(0);
    assert_eq!(
        node_count, 0,
        "empty plan must produce 0 stack nodes"
    );
}

#[test]
fn seed_orchestrator_stack_from_plan_rejects_cyclic_plan() {
    // Given — a plan with a cycle (n1 parent=n2, n2 parent=n1)
    let tmp = tempfile::tempdir().expect("temp dir");
    let session_dir = tmp.path();
    write_empty_orchestrator_changeset(session_dir);

    let plan = StackPlanOutput {
        version: 1,
        prs: vec![
            PlannedPr {
                node_id: "n1".into(),
                title: "A".into(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n2".into()],
                child_recipe: None,
            },
            PlannedPr {
                node_id: "n2".into(),
                title: "B".into(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n1".into()],
                child_recipe: None,
            },
        ],
    };

    // When/Then — a cyclic plan must be rejected
    let result = seed_orchestrator_stack_from_plan(session_dir, &plan);
    assert!(
        result.is_err(),
        "seeding a cyclic plan must return Err — got: {result:?}"
    );

    // …and the changeset must not have been modified (no partial write)
    let cs = read_changeset(session_dir).expect("changeset readable");
    assert!(
        cs.stack.is_none(),
        "changeset.stack must remain None after a rejected cyclic plan"
    );
}

// ensure tempfile is in dev-dependencies
mod _import_check {
    // Compile-time check: the bridge functions are pub and importable.
    #[allow(unused_imports)]
    use tddy_workflow_recipes::orchestrate_pr_stack::{
        execute_stack_merge, execute_stack_repoint, seed_orchestrator_stack_from_plan,
    };
}
