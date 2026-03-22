//! Unit tests for the refactor goal infrastructure.
//!
//! All tests are Red state — they fail to compile because the production
//! functions and types do not exist yet:
//! - `refactor_allowlist()` permission function
//! - `parse_refactor_response()` parser
//! - `RefactorOutput` struct

/// AC (R5): refactor_allowlist includes Write, Edit, and Bash tools.
///
/// Fails to compile until refactor_allowlist() is implemented in permission.rs.
#[test]
fn refactor_allowlist_includes_write_and_bash() {
    use tddy_workflow_recipes::refactor_allowlist;

    let list = refactor_allowlist();

    assert!(
        list.iter().any(|t| t == "Write"),
        "refactor_allowlist must include Write, got: {:?}",
        list
    );
    assert!(
        list.iter().any(|t| t == "Edit"),
        "refactor_allowlist must include Edit, got: {:?}",
        list
    );
    assert!(
        list.iter().any(|t: &String| t.starts_with("Bash")),
        "refactor_allowlist must include at least one Bash tool, got: {:?}",
        list
    );
    assert!(
        list.iter().any(|t| t == "Read"),
        "refactor_allowlist must include Read, got: {:?}",
        list
    );
}

/// AC (R5): parse_refactor_response extracts summary, tasks_completed, tests_passing.
///
/// Fails to compile until parse_refactor_response() and RefactorOutput exist.
#[test]
fn parse_refactor_response_extracts_fields() {
    use tddy_workflow_recipes::parse_refactor_response;

    let input = r#"{"goal":"refactor","summary":"Executed 3 refactoring tasks from refactoring-plan.md. All tests pass.","tasks_completed":3,"tests_passing":true}"#;

    let result = parse_refactor_response(input);
    assert!(
        result.is_ok(),
        "parse_refactor_response should succeed, got: {:?}",
        result
    );

    let output = result.unwrap();
    assert!(!output.summary.is_empty(), "summary must not be empty");
    assert_eq!(output.tasks_completed, 3, "tasks_completed should be 3");
    assert!(output.tests_passing, "tests_passing should be true");
}

/// parse_refactor_response returns Err on empty input.
///
/// Fails to compile until parse_refactor_response() exists.
#[test]
fn parse_refactor_response_fails_on_empty_input() {
    use tddy_workflow_recipes::parse_refactor_response;

    let result = parse_refactor_response("");
    assert!(
        result.is_err(),
        "parse_refactor_response must fail on empty input"
    );
}

/// parse_refactor_response returns Err when goal != "refactor".
///
/// Fails to compile until parse_refactor_response() exists.
#[test]
fn parse_refactor_response_fails_on_wrong_goal() {
    use tddy_workflow_recipes::parse_refactor_response;

    let input =
        r#"{"goal":"green","summary":"Wrong goal.","tasks_completed":0,"tests_passing":false}"#;

    let result = parse_refactor_response(input);
    assert!(
        result.is_err(),
        "parse_refactor_response must fail when goal is not refactor"
    );
}
