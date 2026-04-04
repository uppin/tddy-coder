//! Recipe-driven session-document approval policy (F2/F3).

/// Returns the CLI names that must appear in [`crate::unknown_workflow_recipe_error`] and
/// recipe registries (F5).
pub fn supported_workflow_recipe_cli_names() -> &'static [&'static str] {
    log::debug!(
        "approval_policy::supported_workflow_recipe_cli_names: {:?}",
        &["tdd", "bugfix", "free-prompting", "grill-me", "tdd-small",]
    );
    &["tdd", "bugfix", "free-prompting", "grill-me", "tdd-small"]
}

/// Whether the recipe skips primary session document approval entirely (e.g. free-prompting).
pub fn recipe_should_skip_session_document_approval(recipe_cli_name: &str) -> bool {
    let skip = matches!(recipe_cli_name.trim(), "free-prompting" | "grill-me");
    log::debug!(
        "approval_policy::recipe_should_skip_session_document_approval({:?}) -> {}",
        recipe_cli_name,
        skip
    );
    skip
}
