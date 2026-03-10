//! Goal-specific permission allowlists for Claude Code print mode.
//!
//! Each goal has a predefined list of tools that are auto-approved via --allowedTools.
//! Unexpected requests go to the permission prompt tool (Phase 2) or are denied.

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
        "Bash(git diff *)".to_string(),
        "Bash(git log *)".to_string(),
        "Bash(cargo build *)".to_string(),
        "Bash(cargo check *)".to_string(),
        "Bash(find *)".to_string(),
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
        "Bash(git diff *)".to_string(),
        "Bash(cargo build *)".to_string(),
        "Bash(cargo check *)".to_string(),
        "Bash(cargo test *)".to_string(),
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
        "Bash(cargo *)".to_string(),
        "Bash(git diff *)".to_string(),
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
    ]
}
