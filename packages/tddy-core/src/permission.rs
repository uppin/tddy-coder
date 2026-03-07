//! Goal-specific permission allowlists for Claude Code print mode.
//!
//! Each goal has a predefined list of tools that are auto-approved via --allowedTools.
//! Unexpected requests go to the permission prompt tool (Phase 2) or are denied.

/// Allowlist for the plan goal (read-only analysis).
/// Complements --permission-mode plan.
pub fn plan_allowlist() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
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
        "Bash(cargo *)".to_string(),
        "SemanticSearch".to_string(),
    ]
}
