//! PRD acceptance: plan-pr-stack recipe — resolves from CLI name, maps planned PRs to StackNodes,
//! rejects cyclic plans.
//!
//! **2026-07-01 unified `pr-stack` recipe:** `plan-pr-stack` is now a legacy alias that resolves
//! to the unified `PrStackRecipe` (`recipe.name() == "pr-stack"`), not a standalone
//! `PlanPrStackRecipe` — see `docs/ft/coder/pr-stacking.md#legacy-aliases`.

use tddy_workflow_recipes::plan_pr_stack::{
    planned_prs_into_stack_nodes, validate_stack_plan, PlannedPr, StackPlanOutput,
};
use tddy_workflow_recipes::workflow_recipe_and_manifest_from_cli_name;

#[test]
fn plan_pr_stack_legacy_alias_resolves_to_the_unified_pr_stack_recipe() {
    // When
    let result = workflow_recipe_and_manifest_from_cli_name("plan-pr-stack");

    // Then — the legacy alias resolves to the unified pr-stack recipe, not a standalone one
    let (recipe, _) = result.expect("plan-pr-stack must resolve from CLI name resolver");
    assert_eq!(recipe.name(), "pr-stack");
    assert_eq!(recipe.initial_state().as_str(), "AnalyzeStack");
    assert_eq!(recipe.start_goal().as_str(), "analyze-stack");
}

#[test]
fn planned_prs_into_stack_nodes_maps_three_pr_dag() {
    // Given — a 3-PR DAG plan output matching the plan YAML contract
    let yaml_input = r#"
version: 1
prs:
  - node_id: n1
    title: "Auth token store"
    description: "JWT storage backend"
    branch_suggestion: "feature/auth-store"
    parents: []
    child_recipe: tdd
  - node_id: n2
    title: "Login API"
    description: "POST /login endpoint"
    branch_suggestion: "feature/login-api"
    parents: [n1]
  - node_id: n3
    title: "Dashboard UI"
    description: "React dashboard reading auth"
    branch_suggestion: "feature/dashboard"
    parents: [n1, n2]
"#;
    let plan: StackPlanOutput =
        serde_yaml::from_str(yaml_input.trim()).expect("plan YAML must parse");

    // When
    let nodes = planned_prs_into_stack_nodes(&plan.prs);

    // Then
    assert_eq!(
        nodes.len(),
        3,
        "mapper must produce one StackNode per PlannedPr"
    );
    let n1 = nodes.iter().find(|n| n.node_id == "n1").expect("n1");
    let n2 = nodes.iter().find(|n| n.node_id == "n2").expect("n2");
    let n3 = nodes.iter().find(|n| n.node_id == "n3").expect("n3");
    assert_eq!(n1.parents, Vec::<String>::new());
    assert_eq!(n2.parents, vec!["n1"]);
    assert_eq!(n3.parents, vec!["n1", "n2"]);
    assert_eq!(n1.branch_suggestion.as_deref(), Some("feature/auth-store"));
    assert!(
        n1.session_id.is_none(),
        "no session_id before materialization"
    );
    assert!(n1.branch.is_none(), "no branch before materialization");
}

#[test]
fn validate_stack_plan_rejects_cycle() {
    // Given — cyclic plan (n1 depends on n2, n2 depends on n1)
    let plan = StackPlanOutput {
        version: 1,
        exploration: None,
        prs: vec![
            tddy_workflow_recipes::plan_pr_stack::PlannedPr {
                node_id: "n1".into(),
                title: "A".into(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n2".into()],
                child_recipe: None,
            },
            tddy_workflow_recipes::plan_pr_stack::PlannedPr {
                node_id: "n2".into(),
                title: "B".into(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n1".into()],
                child_recipe: None,
            },
        ],
    };

    // When
    let result = validate_stack_plan(&plan);

    // Then
    assert!(
        result.is_err(),
        "validate_stack_plan must reject cyclic plans"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.to_lowercase().contains("cycle"),
        "validation error must mention cycle; got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Branch-name contract: every PR must carry a branch_suggestion, grouped under one
// `feature/<stack-slug>/<node>` namespace so the whole stack's branches group together and
// "Start session" can create the branch (branch_worktree_intent = new_branch_from_base requires
// a non-empty new_branch_name). See docs/ft/coder/pr-stacking.md.
// ---------------------------------------------------------------------------

/// A PlannedPr with valid defaults — a grouped `feature/todo-app/<node>` branch — so a scenario
/// overrides only the branch field under test.
fn a_planned_pr(node_id: &str) -> PlannedPr {
    PlannedPr {
        node_id: node_id.into(),
        title: format!("PR {node_id}"),
        description: String::new(),
        branch_suggestion: Some(format!("feature/todo-app/{node_id}")),
        parents: vec![],
        child_recipe: None,
    }
}

#[test]
fn validate_stack_plan_rejects_a_pr_without_a_branch_suggestion() {
    // Given — a structurally valid single-PR plan whose only defect is a missing branch name.
    // The pr-stack agent must pre-fill branch_suggestion so "Start session" can open the branch.
    let mut pr = a_planned_pr("scaffold");
    pr.branch_suggestion = None;
    let plan = StackPlanOutput {
        version: 1,
        exploration: None,
        prs: vec![pr],
    };

    // When
    let result = validate_stack_plan(&plan);

    // Then
    let msg = result.expect_err("a plan with a missing branch suggestion must be rejected");
    assert!(
        msg.to_lowercase().contains("branch"),
        "error must name the missing branch; got: {msg}"
    );
    assert!(
        msg.contains("scaffold"),
        "error must identify the offending node; got: {msg}"
    );
}

#[test]
fn validate_stack_plan_rejects_a_branch_not_in_feature_stack_node_shape() {
    // Given — a branch that is present but not in the `feature/<stack>/<node>` form, so it carries
    // no shared stack namespace segment.
    let mut pr = a_planned_pr("scaffold");
    pr.branch_suggestion = Some("todo-scaffold".into());
    let plan = StackPlanOutput {
        version: 1,
        exploration: None,
        prs: vec![pr],
    };

    // When
    let result = validate_stack_plan(&plan);

    // Then
    let msg = result.expect_err("a branch not in feature/<stack>/<node> form must be rejected");
    assert!(
        msg.contains("feature/"),
        "error must state the required feature/<stack>/<node> shape; got: {msg}"
    );
}

#[test]
fn validate_stack_plan_rejects_branches_split_across_stack_namespaces() {
    // Given — two PRs whose branches live under DIFFERENT feature/<stack>/ namespaces
    // (feature/todo-app/... vs feature/other-thing/...), so they would not group as one stack.
    let scaffold = a_planned_pr("scaffold"); // feature/todo-app/scaffold
    let mut core = a_planned_pr("core");
    core.parents = vec!["scaffold".into()];
    core.branch_suggestion = Some("feature/other-thing/core".into());
    let plan = StackPlanOutput {
        version: 1,
        exploration: None,
        prs: vec![scaffold, core],
    };

    // When
    let result = validate_stack_plan(&plan);

    // Then
    let msg = result.expect_err("branches under different stack namespaces must be rejected");
    assert!(
        msg.to_lowercase().contains("namespace"),
        "error must explain the shared-namespace grouping requirement; got: {msg}"
    );
}
