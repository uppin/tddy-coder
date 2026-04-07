//! Workflow recipe resolution for CLI and config (`tdd` | `bugfix`).
//!
//! Delegates to [`tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name`] (single source of truth).

use std::sync::Arc;

use tddy_core::WorkflowRecipe;
pub use tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name;

/// When `--recipe` is omitted and there is no persisted non-empty `recipe` in `changeset.yaml`,
/// this CLI name is used (PRD: default is `free-prompting`, not `tdd`).
pub fn default_unspecified_workflow_recipe_cli_name() -> &'static str {
    log::debug!(
        "default_unspecified_workflow_recipe_cli_name -> {:?}",
        tddy_core::DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME
    );
    log::info!(
        "default unspecified workflow recipe CLI name is {}",
        tddy_core::DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME
    );
    tddy_core::DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME
}

/// Central entry for constructing a [`WorkflowRecipe`] from CLI/config (PRD: recipe factory).
///
/// Delegates to [`resolve_workflow_recipe_from_cli_name`].
#[derive(Debug, Default, Clone, Copy)]
pub struct WorkflowRecipeResolver;

impl WorkflowRecipeResolver {
    pub fn from_cli_name(name: &str) -> Result<Arc<dyn WorkflowRecipe>, String> {
        log::debug!("WorkflowRecipeResolver::from_cli_name(name={:?})", name);
        resolve_workflow_recipe_from_cli_name(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_matches_exported_function() {
        let a = resolve_workflow_recipe_from_cli_name("tdd").expect("tdd");
        let b = WorkflowRecipeResolver::from_cli_name("tdd").expect("tdd via resolver");
        assert_eq!(a.name(), b.name());
    }
}

/// Granular tests for default unspecified recipe resolution.
#[cfg(test)]
mod default_recipe_tests {
    use super::default_unspecified_workflow_recipe_cli_name;

    #[test]
    fn default_unspecified_cli_name_is_free_prompting() {
        assert_eq!(
            default_unspecified_workflow_recipe_cli_name(),
            "free-prompting"
        );
    }
}
