//! Integration tests for output parser.

use tddy_core::output::parse_planning_output;

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
