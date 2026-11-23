//! `#workqueue` 1/14: the `pr-stack` workflow recipe and its legacy aliases are removed. Stacks
//! (work queues) are planned and driven through the project record, its view and the agent tools —
//! never through a recipe. These tests pin the catalogue every recipe surface reads.
//!
//! Feature doc: docs/ft/coder/workflow-recipes.md.

use std::path::Path;

use tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names;
use tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name;

const REMOVED_STACK_RECIPE_NAMES: [&str; 3] = ["pr-stack", "plan-pr-stack", "orchestrate-pr-stack"];

#[test]
fn no_recipe_name_resolves_to_a_stack_recipe() {
    for name in REMOVED_STACK_RECIPE_NAMES {
        // When
        let resolved = resolve_workflow_recipe_from_cli_name(name);

        // Then
        let refusal = resolved
            .err()
            .unwrap_or_else(|| panic!("{name:?} still resolves to a workflow recipe"));
        assert!(
            refusal.contains(name),
            "the unknown-recipe refusal names {name:?}: {refusal}"
        );
    }
}

#[test]
fn the_supported_recipe_list_names_no_stack_recipe() {
    // When
    let supported = supported_workflow_recipe_cli_names();

    // Then
    let still_listed: Vec<&str> = REMOVED_STACK_RECIPE_NAMES
        .into_iter()
        .filter(|name| supported.contains(name))
        .collect();
    assert!(
        still_listed.is_empty(),
        "the supported recipe list still names {still_listed:?}"
    );
}

#[test]
fn the_recipes_crate_holds_no_stack_module() {
    // Given
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    // When
    let present: Vec<&str> = ["work_queue", "plan_pr_stack", "orchestrate_pr_stack"]
        .into_iter()
        .filter(|module| src.join(module).exists() || src.join(format!("{module}.rs")).exists())
        .collect();

    // Then
    assert!(
        present.is_empty(),
        "tddy-workflow-recipes still holds the stack modules {present:?}; what the daemon still \
         uses belongs in the stack crate"
    );
}
