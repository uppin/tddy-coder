//! Lower-level unit tests for validate-refactor infrastructure.
//!
//! All tests are Red state — they fail because the production functions
//! use todo!() and will panic at runtime.

use tddy_core::{parse_validate_refactor_response, validate_refactor_allowlist};

/// validate_refactor_allowlist() returns a non-empty list.
/// Fails in Red because validate_refactor_allowlist calls todo!().
#[test]
fn validate_refactor_allowlist_returns_non_empty_list() {
    let list = validate_refactor_allowlist();
    assert!(
        !list.is_empty(),
        "validate_refactor_allowlist must return tools"
    );
}

/// validate_refactor_allowlist() includes the Agent tool.
/// Fails in Red because validate_refactor_allowlist calls todo!().
#[test]
fn validate_refactor_allowlist_includes_agent() {
    let list = validate_refactor_allowlist();
    assert!(
        list.iter().any(|t| t == "Agent"),
        "validate_refactor_allowlist must include Agent, got: {:?}",
        list
    );
}

/// validate_refactor_allowlist() includes the Write tool.
/// Fails in Red because validate_refactor_allowlist calls todo!().
#[test]
fn validate_refactor_allowlist_includes_write() {
    let list = validate_refactor_allowlist();
    assert!(
        list.iter().any(|t| t == "Write"),
        "validate_refactor_allowlist must include Write, got: {:?}",
        list
    );
}

/// parse_validate_refactor_response returns Err on empty input.
/// Fails in Red because parse_validate_refactor_response calls todo!().
#[test]
fn parse_validate_refactor_response_fails_on_empty_input() {
    let result = parse_validate_refactor_response("");
    assert!(
        result.is_err(),
        "parse_validate_refactor_response must fail on empty input"
    );
}

/// parse_validate_refactor_response returns Err when no structured-response block is present.
/// Fails in Red because parse_validate_refactor_response calls todo!().
#[test]
fn parse_validate_refactor_response_fails_on_missing_block() {
    let result = parse_validate_refactor_response("no block here");
    assert!(
        result.is_err(),
        "parse_validate_refactor_response must fail when block is absent"
    );
}
