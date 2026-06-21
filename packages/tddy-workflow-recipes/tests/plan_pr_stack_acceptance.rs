//! PRD acceptance: plan-pr-stack recipe — resolves from CLI name, maps planned PRs to StackNodes,
//! rejects cyclic plans.

use tddy_workflow_recipes::plan_pr_stack::{
    planned_prs_into_stack_nodes, validate_stack_plan, StackPlanOutput,
};
use tddy_workflow_recipes::workflow_recipe_and_manifest_from_cli_name;

#[test]
fn plan_pr_stack_recipe_resolves() {
    // When
    let result = workflow_recipe_and_manifest_from_cli_name("plan-pr-stack");

    // Then
    let (recipe, _) = result.expect("plan-pr-stack must resolve from CLI name resolver");
    assert_eq!(recipe.name(), "plan-pr-stack");
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
