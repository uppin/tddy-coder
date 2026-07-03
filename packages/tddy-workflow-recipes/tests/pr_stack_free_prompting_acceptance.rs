//! PRD acceptance: the `pr-stack` recipe no longer runs an automatic agentic loop.
//!
//! After planning, the same orchestrator session drops into a single terminal `orchestrate`
//! goal (a free-prompting chat, no `end`/successor edge) and the developer drives the stack
//! by hand through the PR-management tools. The old `begin-orchestrate` / `assess` / `spawn` /
//! `merge` / `repoint` graph nodes and their auto-cycle edges are gone.
//!
//! PRD: docs/ft/coder/pr-stacking.md (2026-07-03 "free-prompting operator loop").

use std::sync::Arc;

use tddy_core::backend::StubBackend;
use tddy_core::changeset::{Changeset, Stack, StackNode};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_workflow_recipes::PrStackRecipe;

fn pr_stack_graph() -> tddy_core::workflow::graph::Graph {
    PrStackRecipe.build_graph(Arc::new(StubBackend::new()))
}

#[test]
fn planning_flows_into_a_terminal_orchestrate_loop() {
    // Given
    let graph = pr_stack_graph();
    let ctx = Context::new();

    // When / Then — write-stack-plan hands off straight to the interactive orchestrate goal
    assert_eq!(
        graph.next_task_id("write-stack-plan", &ctx),
        Some("orchestrate".to_string()),
        "planning must flow into the orchestrate free-prompting loop"
    );
    assert!(
        graph.get_task("orchestrate").is_some(),
        "the graph must contain an 'orchestrate' task"
    );
    assert_eq!(
        graph.next_task_id("orchestrate", &ctx),
        None,
        "orchestrate is terminal (no successor) so FlowRunner pauses for the next prompt"
    );
}

#[test]
fn the_autonomous_assess_spawn_merge_repoint_loop_is_removed_from_the_graph() {
    // Given
    let graph = pr_stack_graph();

    // Then — none of the old auto-loop tasks exist anymore
    for removed in ["begin-orchestrate", "assess", "spawn", "merge", "repoint"] {
        assert!(
            graph.get_task(removed).is_none(),
            "auto-loop task '{removed}' must be removed from the pr-stack graph"
        );
    }
}

#[test]
fn resuming_a_planned_stack_drops_into_the_orchestrate_loop() {
    // Given — the plan exists and the session was closed/reopened
    let recipe = PrStackRecipe;
    let state = WorkflowState::new("StackPlanned");

    // When
    let next = recipe.next_goal_for_state(&state);

    // Then
    assert_eq!(
        next.map(|g| g.as_str().to_string()),
        Some("orchestrate".to_string()),
        "a planned stack resumes into the interactive orchestrate loop, not an auto assess"
    );
}

#[test]
fn a_legacy_orchestrator_stuck_at_init_with_a_populated_stack_resumes_into_orchestrate() {
    // Given — an old session whose state never left "Init" but whose stack already has nodes
    let recipe = PrStackRecipe;
    let state = WorkflowState::new("Init");
    let changeset = Changeset {
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Add token store".to_string(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
                internal_status: None,
            }],
        }),
        ..Changeset::default()
    };

    // When
    let next = recipe.next_goal_for_state_with_changeset(&state, &changeset);

    // Then
    assert_eq!(
        next.map(|g| g.as_str().to_string()),
        Some("orchestrate".to_string()),
        "a mid-flight stack resumes into orchestrate, not the removed assess loop"
    );
}
