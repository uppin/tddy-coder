//! Goal-specific permission allowlists for Claude Code print mode (recipe-owned).
//!
//! Each shipped workflow recipe maps goals to these lists for `--allowedTools`.
//! Backends receive resolved tools via `GoalHints` from the active `WorkflowRecipe`, not this module directly.

/// Allowlist for the plan goal (read-only analysis + clarification).
/// Complements --permission-mode plan.
pub fn plan_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
        "ExitPlanMode".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// Allowlist for the acceptance-tests goal (file edits + cargo test).
/// Complements --permission-mode acceptEdits.
pub fn acceptance_tests_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "AskUserQuestion".to_string(),
        "Bash(cargo *)".to_string(),
        "SemanticSearch".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// Allowlist for the red goal (same as acceptance-tests: file edits + cargo test).
/// Complements --permission-mode acceptEdits.
pub fn red_allowlist() -> Vec<String> {
    acceptance_tests_allowlist()
}

/// Allowlist for the green goal (same as red: file edits + cargo test).
/// Complements --permission-mode acceptEdits.
pub fn green_allowlist() -> Vec<String> {
    acceptance_tests_allowlist()
}

/// Allowlist for the standalone demo goal (same as green: file edits + bash for demo script).
pub fn demo_allowlist() -> Vec<String> {
    green_allowlist()
}

/// Allowlist for the evaluate-changes goal (read-only + git diff/log/find + cargo check/build).
pub fn evaluate_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
        "Bash(git diff *)".to_string(),
        "Bash(git log *)".to_string(),
        "Bash(cargo build *)".to_string(),
        "Bash(cargo check *)".to_string(),
        "Bash(find *)".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// Allowlist for the validate goal (subagent-based): evaluate tools + Agent for spawning subagents + Write.
/// The orchestrator spawns 3 concurrent subagents (validate-tests, validate-prod-ready,
/// analyze-clean-code) via the Agent tool, and each subagent writes its report via Write.
pub fn validate_subagents_allowlist() -> Vec<String> {
    vec![
        "Agent".to_string(),
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
        "Bash(git diff *)".to_string(),
        "Bash(cargo build *)".to_string(),
        "Bash(cargo check *)".to_string(),
        "Bash(cargo test *)".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// Allowlist for the refactor goal (full tool access for executing refactoring tasks).
/// Complements --permission-mode acceptEdits.
pub fn refactor_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
        "Bash(cargo *)".to_string(),
        "Bash(git diff *)".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// Allowlist for the update-docs goal (read PRD/changeset/progress, update feature/dev docs).
/// Complements --permission-mode acceptEdits.
pub fn update_docs_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_allowlist_contains_ask_user_question(allowlist: &[String], name: &str) {
        assert!(
            allowlist.contains(&"AskUserQuestion".to_string()),
            "{} allowlist must include AskUserQuestion for agent clarification flow, got: {:?}",
            name,
            allowlist
        );
    }

    #[test]
    fn all_allowlists_include_ask_user_question() {
        assert_allowlist_contains_ask_user_question(&plan_allowlist(), "plan");
        assert_allowlist_contains_ask_user_question(
            &acceptance_tests_allowlist(),
            "acceptance-tests",
        );
        assert_allowlist_contains_ask_user_question(&red_allowlist(), "red");
        assert_allowlist_contains_ask_user_question(&green_allowlist(), "green");
        assert_allowlist_contains_ask_user_question(&demo_allowlist(), "demo");
        assert_allowlist_contains_ask_user_question(&evaluate_allowlist(), "evaluate");
        assert_allowlist_contains_ask_user_question(&validate_subagents_allowlist(), "validate");
        assert_allowlist_contains_ask_user_question(&refactor_allowlist(), "refactor");
        assert_allowlist_contains_ask_user_question(&update_docs_allowlist(), "update-docs");
    }
}
