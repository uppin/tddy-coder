//! Acceptance tests for workflow goal conditions + session context (PRD Testing Plan).
//!
//! These tests are expected to fail until:
//! - Full TDD graph branches on a generic session key (not `run_demo`) in `tddy-workflow-recipes`
//! - Demo elicitation copy is removed from `tddy-core` presenter (`workflow_runner.rs`)

/// AC + Testing Plan (1): Full TDD graph must branch on a generic session context key.
/// `run_demo` is recipe/demo semantics and must not be the conditional key in the graph builder.
#[test]
fn conditional_edge_uses_session_context_key_not_hardcoded_demo_in_core() {
    let graph_rs = include_str!("../../tddy-workflow-recipes/src/tdd/graph.rs");
    assert!(
        !graph_rs.contains("\"run_demo\"") && !graph_rs.contains("'run_demo'"),
        "build_full_tdd_workflow_graph must use a generic context key (e.g. run_optional_step_x) for the post-green conditional edge, not run_demo"
    );
}

/// AC + Testing Plan (3): User-facing demo branching instructions live in recipes; presenter must not
/// special-case demo-plan.md / GreenComplete demo clarification.
#[test]
fn tdd_recipe_prompts_include_demo_branching_instructions_without_core_importing_demo_semantics() {
    let green_rs = include_str!("../../tddy-workflow-recipes/src/tdd/green.rs");
    assert!(
        green_rs.contains("demo") || green_rs.contains("Demo"),
        "tddy-workflow-recipes green prompt must document demo / optional demo behavior for agents"
    );

    let presenter = include_str!("../../tddy-core/src/presenter/workflow_runner.rs");
    assert!(
        !presenter.contains("demo-plan.md"),
        "tddy-core presenter must not hardcode demo-plan.md checks; recipe-owned prompts/hooks only"
    );
}
