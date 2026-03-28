//! Workflow recipe resolution for CLI and config (`tdd` | `bugfix`).
//!
//! Delegates to [`tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name`] (single source of truth).

use std::sync::Arc;

use tddy_core::WorkflowRecipe;
pub use tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name;

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
