//! Feature prompt `/start-<recipe>` — re-exported from [`tddy_core::feature_start_slash`] (single impl).

pub use tddy_core::{
    feature_slash_menu_start_command_labels,
    next_session_recipe_cli_name_after_start_slash_structured_workflow_complete,
    parse_feature_start_slash_line, remainder_after_start_slash_line,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval_policy;

    #[test]
    fn unit_parse_start_slash_bugfix_yields_bugfix_cli_name() {
        assert_eq!(
            parse_feature_start_slash_line("/start-bugfix"),
            Some(Ok("bugfix".to_string()))
        );
    }

    #[test]
    fn unit_slash_menu_includes_start_prefix_for_each_supported_recipe() {
        let labels = feature_slash_menu_start_command_labels();
        for name in approval_policy::supported_workflow_recipe_cli_names() {
            let expected = format!("/start-{name}");
            assert!(
                labels.iter().any(|l| l == &expected),
                "expected {expected} in {labels:?}"
            );
        }
    }

    #[test]
    fn unit_post_completion_restores_free_prompting_cli_name() {
        assert_eq!(
            next_session_recipe_cli_name_after_start_slash_structured_workflow_complete(),
            "free-prompting"
        );
    }

    #[test]
    fn unit_invalid_start_suffix_lists_supported_names_in_error() {
        let parsed = parse_feature_start_slash_line("/start-not-a-real-recipe-xyz");
        assert!(parsed.is_some());
        let err = parsed.unwrap().unwrap_err();
        assert!(err.contains("tdd") && err.contains("bugfix"), "got: {err}");
    }
}
