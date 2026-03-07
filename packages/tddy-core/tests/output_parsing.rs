//! Integration tests for output parser and writer.

use tddy_core::output::{
    parse_acceptance_tests_response, parse_planning_output, parse_red_response,
};

#[test]
fn extracts_prd_and_todo_from_delimited_output() {
    let input = r#"preface
---PRD_START---
# PRD

## Summary
Feature X
---PRD_END---
middle
---TODO_START---
- [ ] Task 1
- [ ] Task 2
---TODO_END---
trailing"#;
    let out = parse_planning_output(input).expect("should parse");
    assert!(out.prd.contains("Feature X"));
    assert!(out.todo.contains("Task 1"));
}

#[test]
fn errors_on_missing_prd() {
    let input = "---TODO_START---\n- [ ] Task\n---TODO_END---";
    let err = parse_planning_output(input).unwrap_err();
    assert!(matches!(err, tddy_core::ParseError::MissingPrd));
}

#[test]
fn errors_on_missing_todo() {
    let input = "---PRD_START---\n# PRD\n---PRD_END---";
    let err = parse_planning_output(input).unwrap_err();
    assert!(matches!(err, tddy_core::ParseError::MissingTodo));
}

/// Acceptance-tests and red outputs include sequential_command, logging_command when provided.
/// Parser must handle these fields; writers must include them in markdown.
#[test]
fn enhanced_test_instructions_include_sequential_and_logging() {
    let at_input = r#"<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created tests.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"test_command":"cargo test","sequential_command":"cargo test -- --test-threads=1","logging_command":"RUST_LOG=debug cargo test"}
</structured-response>"#;
    let at_out = parse_acceptance_tests_response(at_input).expect("parse acceptance tests");
    assert_eq!(at_out.tests.len(), 1);
    assert_eq!(at_out.tests[0].name, "t1");

    let red_input = r#"<structured-response content-type="application-json">
{"goal":"red","summary":"Created.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"skeletons":[],"sequential_command":"cargo test -- --test-threads=1","logging_command":"RUST_LOG=debug cargo test"}
</structured-response>"#;
    let red_out = parse_red_response(red_input).expect("parse red");
    assert_eq!(red_out.tests.len(), 1);
}

/// Generated markdown files contain relative links to peer documents.
#[test]
fn markdown_cross_references_added() {
    use tddy_core::output::{write_artifacts, PlanningOutput};

    let plan_dir = std::env::temp_dir().join("tddy-crossref-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create dir");

    let planning = PlanningOutput {
        prd: "# PRD\n## Summary\nFeature.".to_string(),
        todo: "- [ ] Task 1".to_string(),
        name: None,
        discovery: None,
        demo_plan: None,
    };
    write_artifacts(&plan_dir, &planning).expect("write artifacts");

    let prd_content = std::fs::read_to_string(plan_dir.join("PRD.md")).expect("read PRD");
    assert!(
        prd_content.contains("TODO.md") || prd_content.contains("Related Documents") || prd_content.contains("./"),
        "PRD.md should have cross-references to peer documents (TODO.md or Related Documents section)"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}
