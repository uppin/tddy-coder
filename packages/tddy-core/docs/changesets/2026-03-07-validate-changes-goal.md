# 2026-03-07 — Validate-Changes Goal

**Type:** Feature

New `--goal validate-changes` for standalone change validation. Goal::Validate, Workflow::validate(), WorkflowState::Validating/Validated. validate_allowlist() with Read, Glob, Grep, SemanticSearch, Bash(git diff/log/find/cargo build/check). parse_validate_response(), write_validation_report(). next_goal_for_state(Validating|Validated) => None. Real-time --conversation-output. parse_red_response uses last structured-response block. (tddy-core)
