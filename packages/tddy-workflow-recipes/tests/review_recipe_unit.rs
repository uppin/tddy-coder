//! Granular red-phase tests for **review** recipe helpers and graph smoke (markers on stderr).

use std::sync::Arc;

use tddy_core::backend::StubBackend;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::GoalId;
use tddy_core::WorkflowRecipe;
use tddy_workflow_recipes::review::{
    merge_base_strategy_documentation, parse_branch_review_output,
};
use tddy_workflow_recipes::review::{ReviewRecipe, ReviewWorkflowHooks};

#[test]
fn merge_base_strategy_documentation_is_non_empty_for_operators() {
    let doc = merge_base_strategy_documentation();
    assert!(
        doc.contains("merge-base") || doc.contains("merge base"),
        "PRD: deterministic merge-base strategy must be documented; got {:?}",
        doc
    );
}

#[test]
fn branch_review_parser_rejects_wrong_goal_like_post_green_review() {
    let json = serde_json::json!({
        "goal": "green",
        "summary": "s",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- x"
    })
    .to_string();
    let e = parse_branch_review_output(&json).unwrap_err();
    assert!(
        e.contains("branch-review") && e.contains("goal"),
        "expected wrong-goal error; got {:?}",
        e
    );
}

#[test]
fn branch_review_parser_rejects_empty_review_body() {
    let json = serde_json::json!({
        "goal": "branch-review",
        "summary": "s",
        "validity_assessment": "ok",
        "review_body_markdown": "   "
    })
    .to_string();
    let e = parse_branch_review_output(&json).unwrap_err();
    assert!(
        e.contains("review_body_markdown") || e.contains("non-empty"),
        "expected empty-body error; got {:?}",
        e
    );
}

#[test]
fn branch_review_parser_accepts_minimal_valid_json_shape() {
    let json = serde_json::json!({
        "goal": "branch-review",
        "summary": "s",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- x"
    })
    .to_string();
    let r = parse_branch_review_output(&json);
    assert!(
        r.is_ok(),
        "Green must parse branch-review JSON; got {:?}",
        r.err()
    );
}

#[test]
fn review_recipe_plan_refinement_goal_is_branch_review() {
    let r = ReviewRecipe;
    assert_eq!(
        r.plan_refinement_goal(),
        GoalId::new("branch-review"),
        "after inspect elicitation, plan refinement must target branch-review submit step"
    );
}

#[test]
fn review_recipe_build_graph_smoke() {
    let r = ReviewRecipe;
    let backend: Arc<dyn tddy_core::backend::CodingBackend> = Arc::new(StubBackend::new());
    let _g = r.build_graph(backend);
}

#[test]
fn review_hooks_before_task_smoke() {
    let hooks = ReviewWorkflowHooks::new(None);
    let ctx = Context::new();
    hooks.before_task("inspect", &ctx).expect("before_task");
}
