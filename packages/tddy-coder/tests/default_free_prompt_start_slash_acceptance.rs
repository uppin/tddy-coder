//! PRD `default-free-prompting-start-slash` — acceptance tests for default recipe and `/start-*` hooks.
//!
//! See session PRD Testing Plan: default recipe, `/start-*` parse, slash menu, completion → free
//! prompting, invalid `/start-` errors.

use serial_test::serial;
use tddy_coder::default_unspecified_workflow_recipe_cli_name;
use tddy_workflow_recipes::{
    approval_policy::supported_workflow_recipe_cli_names, feature_slash_menu_start_command_labels,
    next_session_recipe_cli_name_after_start_slash_structured_workflow_complete,
    parse_feature_start_slash_line,
};

#[test]
#[serial]
fn default_recipe_is_free_prompting_when_unspecified() {
    assert_eq!(
        default_unspecified_workflow_recipe_cli_name(),
        "free-prompting",
        "PRD: when --recipe is omitted and changeset has no recipe, default CLI name must be free-prompting"
    );
}

#[test]
#[serial]
fn start_slash_resolves_to_recipe_cli_name() {
    assert_eq!(
        parse_feature_start_slash_line("/start-bugfix"),
        Some(Ok("bugfix".to_string())),
        "PRD: /start-<cli-name> must resolve to the same string as WorkflowRecipe CLI names (e.g. bugfix)"
    );
    assert_eq!(
        parse_feature_start_slash_line("/start-tdd a todo app"),
        Some(Ok("tdd".to_string())),
        "PRD: text after the recipe token is feature context, not part of the CLI name"
    );
}

#[test]
#[serial]
fn slash_menu_lists_start_recipe_commands() {
    let labels = feature_slash_menu_start_command_labels();
    for name in supported_workflow_recipe_cli_names() {
        let expected = format!("/start-{name}");
        assert!(
            labels.iter().any(|l| l == &expected),
            "slash autocomplete must include {expected}; got {labels:?}"
        );
    }
}

#[test]
#[serial]
fn structured_workflow_completion_restores_free_prompting() {
    assert_eq!(
        next_session_recipe_cli_name_after_start_slash_structured_workflow_complete(),
        "free-prompting",
        "PRD: after WorkflowComplete for a /start-* structured run, the next SubmitFeatureInput uses free-prompting"
    );
}

#[test]
#[serial]
fn invalid_start_slash_surfaces_supported_recipe_list() {
    let parsed = parse_feature_start_slash_line("/start-not-a-real-recipe-xyz");
    assert!(
        parsed.is_some(),
        "invalid /start- suffix must parse as a start-slash command and yield an error (not None)"
    );
    let err = parsed.unwrap().unwrap_err();
    assert!(
        err.contains("tdd") && err.contains("bugfix"),
        "error must list supported names (e.g. tdd, bugfix); got: {err}"
    );
}
