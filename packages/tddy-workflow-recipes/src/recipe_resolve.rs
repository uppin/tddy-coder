//! Single source of truth: CLI recipe name → [`WorkflowRecipe`] + [`SessionArtifactManifest`].
//!
//! Used by `tddy-coder` (recipe only) and `tddy-service` daemon (pair). Keep allowed names and error
//! text in one place when adding recipes.

use std::sync::Arc;

use tddy_core::WorkflowRecipe;

use crate::{
    approval_policy, BugfixRecipe, FreePromptingRecipe, GrillMeRecipe, MergePrRecipe,
    PrStackRecipe, ReviewRecipe, SessionArtifactManifest, TddRecipe, TddSmallRecipe,
};

/// Resolved workflow recipe plus its session-artifact manifest (same concrete type implements both).
pub type WorkflowRecipeAndManifest = (Arc<dyn WorkflowRecipe>, Arc<dyn SessionArtifactManifest>);

/// Error message for an unknown recipe name (CLI, YAML, daemon).
pub fn unknown_workflow_recipe_error(name: &str) -> String {
    let expected = approval_policy::supported_workflow_recipe_cli_names()
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(", ");
    log::debug!(
        "unknown_workflow_recipe_error: name={:?} expected_one_of=[{}]",
        name,
        expected
    );
    format!(
        r#"unknown workflow recipe {:?} (expected one of: {})"#,
        name, expected
    )
}

/// Same as [`workflow_recipe_and_manifest_from_cli_name`] but only the workflow trait object.
pub fn resolve_workflow_recipe_from_cli_name(
    name: &str,
) -> Result<Arc<dyn WorkflowRecipe>, String> {
    workflow_recipe_and_manifest_from_cli_name(name).map(|(r, _)| r)
}

/// Map a recipe name from `--recipe`, YAML, or `StartSession.recipe` to the active recipe **and**
/// its [`SessionArtifactManifest`] (same concrete type implements both traits).
pub fn workflow_recipe_and_manifest_from_cli_name(
    name: &str,
) -> Result<WorkflowRecipeAndManifest, String> {
    let n = name.trim();
    log::debug!(
        "workflow_recipe_and_manifest_from_cli_name: input={:?}",
        name
    );
    match n {
        "" | "tdd" => {
            log::info!("workflow recipe resolved: tdd (TddRecipe)");
            let r: Arc<TddRecipe> = Arc::new(TddRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "bugfix" => {
            log::info!("workflow recipe resolved: bugfix (BugfixRecipe)");
            let r: Arc<BugfixRecipe> = Arc::new(BugfixRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "free-prompting" => {
            log::info!("workflow recipe resolved: free-prompting (FreePromptingRecipe)");
            let r: Arc<FreePromptingRecipe> = Arc::new(FreePromptingRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "grill-me" => {
            log::info!("workflow recipe resolved: grill-me (GrillMeRecipe)");
            let r: Arc<GrillMeRecipe> = Arc::new(GrillMeRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "tdd-small" => {
            log::info!("workflow recipe resolved: tdd-small (TddSmallRecipe)");
            let r: Arc<TddSmallRecipe> = Arc::new(TddSmallRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "review" => {
            log::info!("workflow recipe resolved: review (ReviewRecipe)");
            let r: Arc<ReviewRecipe> = Arc::new(ReviewRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "merge-pr" => {
            log::info!("workflow recipe resolved: merge-pr (MergePrRecipe)");
            let r: Arc<MergePrRecipe> = Arc::new(MergePrRecipe);
            Ok((
                r.clone() as Arc<dyn WorkflowRecipe>,
                r as Arc<dyn SessionArtifactManifest>,
            ))
        }
        "pr-stack" | "plan-pr-stack" | "orchestrate-pr-stack" => {
            log::info!(
                "workflow recipe resolved: pr-stack (PrStackRecipe) [requested as {:?}]",
                n
            );
            let r = PrStackRecipe;
            Ok((Arc::new(r), Arc::new(r)))
        }
        other => Err(unknown_workflow_recipe_error(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_recipes_are_distinct_by_name() {
        // When
        let (bugfix, _) =
            workflow_recipe_and_manifest_from_cli_name("bugfix").expect("resolve bugfix");
        let (tdd, _) = workflow_recipe_and_manifest_from_cli_name("tdd").expect("resolve tdd");

        // Then
        assert_ne!(bugfix.name(), tdd.name());
    }

    #[test]
    fn resolver_matches_free_function_for_tdd() {
        // When
        let a = resolve_workflow_recipe_from_cli_name("tdd").expect("tdd");
        let b = workflow_recipe_and_manifest_from_cli_name("tdd")
            .expect("pair")
            .0;

        // Then
        assert_eq!(a.name(), b.name());
        let ga = a.start_goal();
        let gb = b.start_goal();
        assert_eq!(ga.as_str(), gb.as_str());
    }

    #[test]
    fn manifest_matches_recipe_for_tdd() {
        // When
        let (recipe, manifest) = workflow_recipe_and_manifest_from_cli_name("tdd").expect("tdd");

        // Then
        assert_eq!(recipe.name(), "tdd");
        assert!(!manifest.known_artifacts().is_empty());
    }

    #[test]
    fn resolver_resolves_free_prompting() {
        // When
        let (r, _) = workflow_recipe_and_manifest_from_cli_name("free-prompting").expect("resolve");

        // Then
        assert_eq!(r.name(), "free-prompting");
        assert_eq!(r.start_goal().as_str(), "prompting");
    }

    #[test]
    fn resolver_resolves_grill_me() {
        // When
        let (r, _) = workflow_recipe_and_manifest_from_cli_name("grill-me").expect("resolve");

        // Then
        assert_eq!(r.name(), "grill-me");
        assert_eq!(r.start_goal().as_str(), "grill");
    }

    // -------------------------------------------------------------------
    // pr-stack consolidation: "pr-stack" is the canonical name; the two
    // legacy CLI names remain accepted as aliases that resolve to the
    // same unified recipe (docs/dev/1-WIP/pr-stack-workflow-views.md).
    // -------------------------------------------------------------------

    #[test]
    fn resolver_resolves_the_canonical_pr_stack_name() {
        // When
        let result = workflow_recipe_and_manifest_from_cli_name("pr-stack");

        // Then
        let (recipe, _) = result.expect("\"pr-stack\" should resolve to the unified recipe");
        assert_eq!(recipe.name(), "pr-stack");
    }

    #[test]
    fn resolver_maps_the_legacy_plan_pr_stack_alias_to_the_unified_pr_stack_recipe() {
        // When
        let (recipe, _) = workflow_recipe_and_manifest_from_cli_name("plan-pr-stack")
            .expect("legacy alias must still resolve");

        // Then — same recipe as the canonical name, not the old PlanPrStackRecipe
        assert_eq!(recipe.name(), "pr-stack");
    }

    #[test]
    fn resolver_maps_the_legacy_orchestrate_pr_stack_alias_to_the_unified_pr_stack_recipe() {
        // When
        let (recipe, _) = workflow_recipe_and_manifest_from_cli_name("orchestrate-pr-stack")
            .expect("legacy alias must still resolve");

        // Then — same recipe as the canonical name, not the old OrchestratePrStackRecipe
        assert_eq!(recipe.name(), "pr-stack");
    }
}
