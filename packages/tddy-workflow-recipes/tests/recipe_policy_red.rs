//! Red-phase unit tests: policy helpers + [`FreePromptingRecipe`] metadata (granular, fast).
//!
//! These intentionally fail until Green registers `free-prompting`, corrects skeleton metadata,
//! and implements `approval_policy` tables.

use tddy_core::WorkflowRecipe;
use tddy_workflow_recipes::approval_policy;
use tddy_workflow_recipes::FreePromptingRecipe;

#[test]
fn supported_cli_names_includes_free_prompting() {
    let names = approval_policy::supported_workflow_recipe_cli_names();
    assert!(
        names.contains(&"free-prompting"),
        "F5: supported CLI names must include free-prompting for resolver/daemon parity: {:?}",
        names
    );
}

#[test]
fn free_prompting_skips_session_document_approval_per_policy_table() {
    assert!(
        approval_policy::recipe_should_skip_session_document_approval("free-prompting"),
        "free-prompting must skip session document approval when policy says so (F2/F3)"
    );
}

#[test]
fn free_prompting_recipe_exposes_prompting_goal_and_state() {
    let r = FreePromptingRecipe;
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
