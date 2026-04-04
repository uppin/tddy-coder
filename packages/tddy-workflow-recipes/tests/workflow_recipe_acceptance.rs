//! PRD acceptance: resolver + free-prompting recipe metadata (`workflow-free-prompting-approval`).
//!
//! These tests are expected to fail until `free-prompting` is registered and `unknown_workflow_recipe_error`
//! lists every supported CLI recipe name.

use tddy_workflow_recipes::{
    unknown_workflow_recipe_error, workflow_recipe_and_manifest_from_cli_name,
};

#[test]
fn recipe_resolve_accepts_free_prompting_and_rejects_unknown() {
    let err = unknown_workflow_recipe_error("totally-unknown-recipe");
    assert!(
        err.contains("tdd")
            && err.contains("bugfix")
            && err.contains("free-prompting")
            && err.contains("grill-me"),
        "unknown recipe errors must list every supported workflow recipe: {}",
        err
    );
    assert!(
        workflow_recipe_and_manifest_from_cli_name("totally-unknown-recipe").is_err(),
        "unknown names must not resolve"
    );
    let resolved = workflow_recipe_and_manifest_from_cli_name("free-prompting");
    assert!(
        resolved.is_ok(),
        "free-prompting must resolve from CLI/YAML/daemon recipe field: {:?}",
        resolved.err()
    );
}

#[test]
fn free_prompting_recipe_resolves_and_reports_prompting_state() {
    let (recipe, _manifest) = workflow_recipe_and_manifest_from_cli_name("free-prompting")
        .expect("free-prompting must resolve");
    assert_eq!(recipe.name(), "free-prompting");
    assert_eq!(
        recipe.start_goal().as_str(),
        "prompting",
        "free-prompting must use a single primary loop goal id aligned with the Prompting task"
    );
    assert_eq!(
        recipe.initial_state().as_str(),
        "Prompting",
        "free-prompting must expose Prompting as the initial workflow state string"
    );
}

#[test]
fn grill_me_recipe_resolves_and_reports_grill_me_state() {
    let (recipe, manifest) =
        workflow_recipe_and_manifest_from_cli_name("grill-me").expect("grill-me must resolve");
    assert_eq!(recipe.name(), "grill-me");
    assert_eq!(recipe.start_goal().as_str(), "grill");
    assert_eq!(recipe.initial_state().as_str(), "Grill");
    assert!(
        !recipe.uses_primary_session_document(),
        "grill-me v1 skips primary session document approval gate"
    );
    assert!(
        manifest
            .default_artifacts()
            .get("grill_brief")
            .map(|s| s.as_str())
            == Some("grill-me-brief.md"),
        "manifest must register grill-me-brief.md under grill_brief key"
    );
}
