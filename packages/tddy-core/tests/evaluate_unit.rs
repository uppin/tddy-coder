//! Lower-level unit tests for evaluate-changes infrastructure.
//!
//! All tests are Red state — they fail because the production functions
//! use todo!() and will panic at runtime.

use tddy_core::{evaluate_allowlist, parse_evaluate_response};

/// evaluate_allowlist() returns a non-empty list.
/// Fails in Red because evaluate_allowlist calls todo!().
#[test]
fn evaluate_allowlist_returns_non_empty_list() {
    let list = evaluate_allowlist();
    assert!(!list.is_empty(), "evaluate_allowlist must return tools");
}

/// evaluate_allowlist() includes Read tool.
/// Fails in Red because evaluate_allowlist calls todo!().
#[test]
fn evaluate_allowlist_includes_read() {
    let list = evaluate_allowlist();
    assert!(
        list.iter().any(|t| t == "Read"),
        "evaluate_allowlist must include Read, got: {:?}",
        list
    );
}

/// evaluate_allowlist() includes a git diff bash entry.
/// Fails in Red because evaluate_allowlist calls todo!().
#[test]
fn evaluate_allowlist_includes_git_diff() {
    let list = evaluate_allowlist();
    assert!(
        list.iter().any(|t| t.contains("git diff")),
        "evaluate_allowlist must include Bash(git diff *), got: {:?}",
        list
    );
}

/// parse_evaluate_response returns Err on empty string.
/// Fails in Red because parse_evaluate_response calls todo!().
#[test]
fn parse_evaluate_response_fails_on_empty_input() {
    let result = parse_evaluate_response("");
    assert!(
        result.is_err(),
        "parse_evaluate_response must fail on empty input"
    );
}

/// parse_evaluate_response returns Err when no structured-response block is present.
/// Fails in Red because parse_evaluate_response calls todo!().
#[test]
fn parse_evaluate_response_fails_on_missing_block() {
    let result = parse_evaluate_response("no structured response here");
    assert!(
        result.is_err(),
        "parse_evaluate_response must fail when block is absent"
    );
}
