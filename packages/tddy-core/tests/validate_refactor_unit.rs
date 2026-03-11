//! Unit tests for validate subagent infrastructure.

use tddy_core::{parse_validate_subagents_response, validate_subagents_allowlist};

/// validate_subagents_allowlist() returns a non-empty list.
#[test]
fn validate_subagents_allowlist_returns_non_empty_list() {
    let list = validate_subagents_allowlist();
    assert!(
        !list.is_empty(),
        "validate_subagents_allowlist must return tools"
    );
}

/// validate_subagents_allowlist() includes the Agent tool.
#[test]
fn validate_subagents_allowlist_includes_agent() {
    let list = validate_subagents_allowlist();
    assert!(
        list.iter().any(|t| t == "Agent"),
        "validate_subagents_allowlist must include Agent, got: {:?}",
        list
    );
}

/// validate_subagents_allowlist() includes the Write tool.
#[test]
fn validate_subagents_allowlist_includes_write() {
    let list = validate_subagents_allowlist();
    assert!(
        list.iter().any(|t| t == "Write"),
        "validate_subagents_allowlist must include Write, got: {:?}",
        list
    );
}

/// parse_validate_subagents_response returns Err on empty input.
#[test]
fn parse_validate_subagents_response_fails_on_empty_input() {
    let result = parse_validate_subagents_response("");
    assert!(
        result.is_err(),
        "parse_validate_subagents_response must fail on empty input"
    );
}

/// parse_validate_subagents_response returns Err when no structured-response block is present.
#[test]
fn parse_validate_subagents_response_fails_on_missing_block() {
    let result = parse_validate_subagents_response("no block here");
    assert!(
        result.is_err(),
        "parse_validate_subagents_response must fail when block is absent"
    );
}

// ── parse_validate_subagents_response ─────────────────────────────────────────

/// parse_validate_subagents_response extracts refactoring_plan_written field.
#[test]
fn parse_validate_subagents_response_with_refactoring_plan() {
    use tddy_core::parse_validate_subagents_response;

    let input = r#"{"goal":"validate","summary":"All 3 subagents completed. Refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

    let result = parse_validate_subagents_response(input);
    assert!(
        result.is_ok(),
        "parse_validate_subagents_response should succeed, got: {:?}",
        result
    );

    let output = result.unwrap();
    assert!(
        output.refactoring_plan_written,
        "refactoring_plan_written should be true"
    );
    assert!(output.tests_report_written);
    assert!(output.prod_ready_report_written);
    assert!(output.clean_code_report_written);
}
