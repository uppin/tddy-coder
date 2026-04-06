//! `/start-<recipe>` feature-prompt commands and default free-prompting recipe name (PRD).
//!
//! Kept in `tddy-core` so the presenter and [`crate::agent_skills::slash_menu_entries`] can use the
//! same contract as `tddy-workflow-recipes` without a circular dependency.

/// CLI name for **free prompting** when no recipe is explicitly selected (matches changeset + resolver).
pub const DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME: &str = "free-prompting";

/// Shipped workflow recipe CLI names — **keep in sync** with
/// `tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names`.
pub const SHIPPED_WORKFLOW_RECIPE_CLI_NAMES: &[&str] = &[
    "tdd",
    "bugfix",
    "free-prompting",
    "grill-me",
    "tdd-small",
    "review",
    "merge-pr",
];

fn unknown_workflow_recipe_error(name: &str) -> String {
    let expected = SHIPPED_WORKFLOW_RECIPE_CLI_NAMES
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"unknown workflow recipe {:?} (expected one of: {})"#,
        name, expected
    )
}

/// Slash autocomplete labels: one `/start-<cli-name>` per shipped recipe (order matches
/// [`SHIPPED_WORKFLOW_RECIPE_CLI_NAMES`]).
pub fn feature_slash_menu_start_command_labels() -> Vec<String> {
    log::debug!("feature_slash_menu_start_command_labels: building from shipped CLI names");
    let labels: Vec<String> = SHIPPED_WORKFLOW_RECIPE_CLI_NAMES
        .iter()
        .map(|name| format!("/start-{name}"))
        .collect();
    log::debug!(
        "feature_slash_menu_start_command_labels: count={} first={:?}",
        labels.len(),
        labels.first()
    );
    labels
}

/// Next session recipe CLI name after `WorkflowComplete` for a structured run started via `/start-*`.
#[must_use]
pub fn next_session_recipe_cli_name_after_start_slash_structured_workflow_complete() -> &'static str
{
    log::debug!(
        "next_session_recipe_cli_name_after_start_slash_structured_workflow_complete -> {:?}",
        DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME
    );
    DEFAULT_UNSPECIFIED_WORKFLOW_RECIPE_CLI_NAME
}

/// Parse `/start-<cli-recipe-name>` from a feature prompt line.
///
/// Returns [`None`] if the line does not start with `/start-` (after trim). Otherwise [`Some`]`(Ok(name))`
/// or [`Some`]`(Err(..))` aligned with resolver/unknown-recipe messaging.
pub fn parse_feature_start_slash_line(line: &str) -> Option<Result<String, String>> {
    log::debug!(
        "parse_feature_start_slash_line: line_len={} prefix_ok={}",
        line.len(),
        line.trim().starts_with("/start-")
    );
    let line = line.trim();
    let rest = line.strip_prefix("/start-")?;
    let suffix = rest.trim();
    if suffix.is_empty() {
        log::debug!("parse_feature_start_slash_line: empty suffix");
        return Some(Err("empty /start- recipe suffix".to_string()));
    }
    if SHIPPED_WORKFLOW_RECIPE_CLI_NAMES
        .iter()
        .any(|n| *n == suffix)
    {
        log::debug!("parse_feature_start_slash_line: ok suffix={:?}", suffix);
        Some(Ok(suffix.to_string()))
    } else {
        log::debug!(
            "parse_feature_start_slash_line: unknown suffix={:?}",
            suffix
        );
        Some(Err(unknown_workflow_recipe_error(suffix)))
    }
}

/// Text after `/start-<cli>` on the same line (trimmed). Empty if the line is only `/start-<cli>`.
#[must_use]
pub fn remainder_after_start_slash_line(line: &str, cli_name: &str) -> String {
    let line = line.trim();
    let prefix = format!("/start-{cli_name}");
    line.strip_prefix(&prefix)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bugfix() {
        assert_eq!(
            parse_feature_start_slash_line("/start-bugfix"),
            Some(Ok("bugfix".to_string()))
        );
    }

    #[test]
    fn parse_unknown_lists_names() {
        let e = parse_feature_start_slash_line("/start-nope")
            .unwrap()
            .unwrap_err();
        assert!(e.contains("tdd") && e.contains("bugfix"));
    }
}
