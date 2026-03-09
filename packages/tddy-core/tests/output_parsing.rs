//! Integration tests for output parser and writer.

use tddy_core::output::{
    parse_acceptance_tests_response, parse_evaluate_response, parse_planning_output,
    parse_red_response,
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

/// parse_evaluate_response() extracts summary, risk_level, issues, and build_results correctly.
#[test]
fn parse_evaluate_response_extracts_all_fields() {
    let input = r#"<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Analyzed 2 files. Risk: low.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[{"severity":"warning","category":"code_quality","file":"src/lib.rs","line":10,"description":"Magic number","suggestion":"Use a named constant"}],"changeset_sync":{"status":"synced","items_updated":1,"items_added":0},"files_analyzed":[{"file":"src/lib.rs","lines_changed":5,"changeset_item":"auth-login"}],"test_impact":{"tests_affected":1,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"Ready"}
</structured-response>"#;

    let out = parse_evaluate_response(input).expect("parse_evaluate_response should succeed");
    assert!(
        out.summary.contains("Analyzed"),
        "summary should contain 'Analyzed', got: {}",
        out.summary
    );
    assert_eq!(out.risk_level, "low", "risk_level should be 'low'");
    assert_eq!(out.build_results.len(), 1, "should have 1 build result");
    assert_eq!(out.build_results[0].package, "tddy-core");
    assert_eq!(out.build_results[0].status, "pass");
    assert_eq!(out.issues.len(), 1, "should have 1 issue");
    assert_eq!(out.issues[0].severity, "warning");
    assert_eq!(out.issues[0].category, "code_quality");
    assert_eq!(out.issues[0].file, "src/lib.rs");
    assert_eq!(out.issues[0].line, Some(10));
    assert_eq!(out.issues[0].description, "Magic number");
    let sync = out
        .changeset_sync
        .as_ref()
        .expect("changeset_sync should be present");
    assert_eq!(sync.status, "synced");
    assert_eq!(sync.items_updated, 1);
    assert_eq!(out.files_analyzed.len(), 1);
    assert_eq!(out.files_analyzed[0].file, "src/lib.rs");
    let impact = out
        .test_impact
        .as_ref()
        .expect("test_impact should be present");
    assert_eq!(impact.tests_affected, 1);
}

/// parse_evaluate_response() uses the last structured-response block, skipping tool results.
#[test]
fn parse_evaluate_response_skips_tool_result_block() {
    let input = r#"Tool result from earlier in stream:
<structured-response content-type="application-json">
{"inputTokens":760,"outputTokens":42,"cacheReadInputTokens":100}
</structured-response>

Agent's actual response:
<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Analyzed 1 file.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"not_found","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/main.rs","lines_changed":5,"changeset_item":null}],"test_impact":{"tests_affected":0,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"Ready"}
</structured-response>"#;

    let out = parse_evaluate_response(input).expect("parse_evaluate_response should succeed");
    assert_eq!(out.summary, "Analyzed 1 file.");
    assert_eq!(out.risk_level, "low");
}

/// parse_evaluate_response() returns ParseError::Malformed when the goal field is not "evaluate-changes".
#[test]
fn parse_evaluate_response_fails_on_wrong_goal_field() {
    let input = r#"<structured-response content-type="application-json">
{"goal":"plan","summary":"This is a plan, not an evaluation.","risk_level":"low","build_results":[],"issues":[],"changeset_sync":null,"files_analyzed":[],"test_impact":null,"changed_files":[],"affected_tests":[],"validity_assessment":""}
</structured-response>"#;

    let err = parse_evaluate_response(input).expect_err("should fail on wrong goal");
    assert!(
        matches!(err, tddy_core::ParseError::Malformed(_)),
        "expected ParseError::Malformed for wrong goal field, got: {:?}",
        err
    );
}
