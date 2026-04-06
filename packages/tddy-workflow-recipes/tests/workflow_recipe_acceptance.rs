//! PRD acceptance: resolver + free-prompting recipe metadata (`workflow-free-prompting-approval`).
//!
//! These tests are expected to fail until `free-prompting` is registered and `unknown_workflow_recipe_error`
//! lists every supported CLI recipe name.

use tddy_core::GoalId;
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
            && err.contains("grill-me")
            && err.contains("tdd-small")
            && err.contains("review")
            && err.contains("merge-pr"),
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

/// PRD: `tdd-small` resolves to a distinct recipe + manifest; `recipe.name()` is `tdd-small`.
#[test]
fn resolve_tdd_small_recipe() {
    let resolved = workflow_recipe_and_manifest_from_cli_name("tdd-small");
    let (recipe, _manifest) =
        resolved.expect("tdd-small must resolve via workflow_recipe_and_manifest_from_cli_name");
    assert_eq!(recipe.name(), "tdd-small");
}

/// PRD: `workflow_recipe_and_manifest_from_cli_name("review")` succeeds; `ReviewRecipe` contract for name, manifest, elicitation vs submit.
#[test]
fn workflow_recipe_resolves_review() {
    let (recipe, manifest) =
        workflow_recipe_and_manifest_from_cli_name("review").expect("review must resolve");
    assert_eq!(recipe.name(), "review");
    assert!(
        !recipe.start_goal().as_str().is_empty(),
        "ReviewRecipe must expose a non-empty start goal id"
    );
    assert_eq!(
        manifest
            .default_artifacts()
            .get("review")
            .map(|s| s.as_str()),
        Some("review.md"),
        "SessionArtifactManifest must map review → review.md for discovery / UI"
    );
}

/// PRD: first turn is elicitation (no structured submit), like grill-me **Grill**; completion uses `branch-review` submit.
#[test]
fn review_recipe_elicitation_parity_with_grill_me() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("review").expect("review must resolve");
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&recipe.start_goal()),
        "review start goal must allow elicitation without structured submit (grill-me parity)"
    );
    assert!(
        recipe.goal_requires_tddy_tools_submit(&GoalId::new("branch-review")),
        "branch-review goal must require tddy-tools submit to persist review.md"
    );
}
