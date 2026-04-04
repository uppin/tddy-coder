//! Single source of truth: CLI recipe name → [`WorkflowRecipe`] + [`SessionArtifactManifest`].
//!
//! Used by `tddy-coder` (recipe only) and `tddy-service` daemon (pair). Keep allowed names and error
//! text in one place when adding recipes.

use std::sync::Arc;

use tddy_core::WorkflowRecipe;

use crate::{
    approval_policy, BugfixRecipe, FreePromptingRecipe, GrillMeRecipe, SessionArtifactManifest,
    TddRecipe, TddSmallRecipe,
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
        other => Err(unknown_workflow_recipe_error(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_recipes_are_distinct_by_name() {
        let (bugfix, _) =
            workflow_recipe_and_manifest_from_cli_name("bugfix").expect("resolve bugfix");
        let (tdd, _) = workflow_recipe_and_manifest_from_cli_name("tdd").expect("resolve tdd");
        assert_ne!(bugfix.name(), tdd.name());
    }

    #[test]
    fn resolver_matches_free_function_for_tdd() {
        let a = resolve_workflow_recipe_from_cli_name("tdd").expect("tdd");
        let b = workflow_recipe_and_manifest_from_cli_name("tdd")
            .expect("pair")
            .0;
        assert_eq!(a.name(), b.name());
        let ga = a.start_goal();
        let gb = b.start_goal();
        assert_eq!(ga.as_str(), gb.as_str());
    }

    #[test]
    fn manifest_matches_recipe_for_tdd() {
        let (recipe, manifest) = workflow_recipe_and_manifest_from_cli_name("tdd").expect("tdd");
        assert_eq!(recipe.name(), "tdd");
        assert!(!manifest.known_artifacts().is_empty());
    }

    #[test]
    fn resolver_resolves_free_prompting() {
        let (r, _) = workflow_recipe_and_manifest_from_cli_name("free-prompting").expect("resolve");
        assert_eq!(r.name(), "free-prompting");
        assert_eq!(r.start_goal().as_str(), "prompting");
    }

    #[test]
    fn resolver_resolves_grill_me() {
        let (r, _) = workflow_recipe_and_manifest_from_cli_name("grill-me").expect("resolve");
        assert_eq!(r.name(), "grill-me");
        assert_eq!(r.start_goal().as_str(), "grill");
    }
}
