//! Recipe-driven session-document approval policy (F2/F3).

/// Returns the CLI names that must appear in [`crate::unknown_workflow_recipe_error`] and
/// recipe registries (F5).
pub fn supported_workflow_recipe_cli_names() -> &'static [&'static str] {
    log::debug!(
        "approval_policy::supported_workflow_recipe_cli_names: {:?}",
        &[
            "tdd",
            "bugfix",
            "free-prompting",
            "grill-me",
            "tdd-small",
            "review",
            "merge-pr",
            "pr-stack",
            "plan-pr-stack",
            "orchestrate-pr-stack",
        ]
    );
    &[
        "tdd",
        "bugfix",
        "free-prompting",
        "grill-me",
        "tdd-small",
        "review",
        "merge-pr",
        "pr-stack",
        "plan-pr-stack",
        "orchestrate-pr-stack",
    ]
}

/// Whether the recipe skips primary session document approval entirely (e.g. free-prompting).
pub fn recipe_should_skip_session_document_approval(recipe_cli_name: &str) -> bool {
    let skip = matches!(
        recipe_cli_name.trim(),
        "free-prompting"
            | "grill-me"
            | "review"
            | "merge-pr"
            | "pr-stack"
            | "plan-pr-stack"
            | "orchestrate-pr-stack"
    );
    log::debug!(
        "approval_policy::recipe_should_skip_session_document_approval({:?}) -> {}",
        recipe_cli_name,
        skip
    );
    skip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_recipe_names_include_the_canonical_pr_stack_name() {
        // When
        let names = supported_workflow_recipe_cli_names();

        // Then — "pr-stack" is accepted alongside the two legacy aliases it consolidates
        assert!(
            names.contains(&"pr-stack"),
            "expected \"pr-stack\" in {names:?}"
        );
        assert!(
            names.contains(&"plan-pr-stack"),
            "legacy alias must remain accepted"
        );
        assert!(
            names.contains(&"orchestrate-pr-stack"),
            "legacy alias must remain accepted"
        );
    }

    #[test]
    fn pr_stack_skips_primary_session_document_approval_like_its_legacy_aliases() {
        // When / Then — pr-stack has no PRD-style document approval gate, same as plan-pr-stack
        // and orchestrate-pr-stack did before consolidation.
        assert!(recipe_should_skip_session_document_approval("pr-stack"));
    }
}
