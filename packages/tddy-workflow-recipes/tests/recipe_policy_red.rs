//! Red-phase unit tests: policy helpers + [`FreePromptingRecipe`] metadata (granular, fast).
//!
//! These intentionally fail until Green registers `free-prompting`, corrects skeleton metadata,
//! and implements `approval_policy` tables.

use tddy_core::{GoalId, WorkflowRecipe};
use tddy_workflow_recipes::approval_policy;
use tddy_workflow_recipes::{FreePromptingRecipe, GrillMeRecipe};

#[test]
fn supported_cli_names_includes_free_prompting_and_grill_me() {
    // When
    let names = approval_policy::supported_workflow_recipe_cli_names();

    // Then
    assert!(names.contains(&"free-prompting"), "F5: must include free-prompting: {:?}", names);
    assert!(names.contains(&"grill-me"), "F5: must include grill-me: {:?}", names);
    assert!(names.contains(&"tdd-small"), "F5: must include tdd-small: {:?}", names);
    assert!(names.contains(&"review"), "F5: must include review: {:?}", names);
    assert!(names.contains(&"merge-pr"), "F5: must include merge-pr: {:?}", names);
}

#[test]
fn free_prompting_skips_session_document_approval_per_policy_table() {
    // When / Then
    assert!(
        approval_policy::recipe_should_skip_session_document_approval("free-prompting"),
        "free-prompting must skip session document approval when policy says so (F2/F3)"
    );
}

#[test]
fn grill_me_skips_session_document_approval_per_policy_table() {
    // When / Then
    assert!(
        approval_policy::recipe_should_skip_session_document_approval("grill-me"),
        "grill-me v1 must skip session document approval (same class as free-prompting)"
    );
}

#[test]
fn review_skips_session_document_approval_per_policy_table() {
    // When / Then
    assert!(
        approval_policy::recipe_should_skip_session_document_approval("review"),
        "review must skip primary session document approval (grill-me class)"
    );
}

#[test]
fn free_prompting_recipe_exposes_prompting_goal_and_state() {
    // Given
    let r = FreePromptingRecipe;

    // Then
    assert_eq!(r.name(), "free-prompting");
    assert_eq!(
        r.start_goal().as_str(),
        "prompting",
        "start goal must be the primary loop id (not the TDD plan id)"
    );
    assert_eq!(
        r.initial_state().as_str(),
        "Prompting",
        "initial workflow state string must be Prompting (A1)"
    );
    assert!(
        !r.uses_primary_session_document(),
        "free-prompting disables primary session document approval when policy skips it"
    );
}

#[test]
fn grill_me_recipe_exposes_grill_me_goal_and_state() {
    // Given
    let r = GrillMeRecipe;

    // Then
    assert_eq!(r.name(), "grill-me");
    assert_eq!(r.start_goal().as_str(), "grill");
    assert_eq!(r.initial_state().as_str(), "Grill");
    assert!(!r.uses_primary_session_document());
    assert!(r.goal_requires_session_dir(&GoalId::new("grill")));
    assert!(r.goal_requires_session_dir(&GoalId::new("create-plan")));
    assert!(
        !r.goal_requires_tddy_tools_submit(&GoalId::new("grill")),
        "grill goal must not require tddy-tools submit"
    );
    assert!(
        !r.goal_requires_tddy_tools_submit(&GoalId::new("create-plan")),
        "create-plan goal must not require tddy-tools submit"
    );
}
