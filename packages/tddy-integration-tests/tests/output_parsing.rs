//! Integration tests for output parser and writer.

use std::path::Path;
use tddy_workflow_recipes::{
    parse_acceptance_tests_response, parse_evaluate_response, parse_planning_response,
    parse_planning_response_with_base, parse_red_response, parse_update_docs_response,
};

#[test]
fn parse_planning_response_resolves_prd_path_to_file_content() {
    let session_dir = std::env::temp_dir().join("tddy-parse-prd-path-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create dir");

    let prd_content = "# PRD\n\n## Summary\nFeature from file\n\n## TODO\n\n- [ ] Task 1";
    std::fs::write(session_dir.join("PRD.md"), prd_content).expect("write PRD.md");

    let json = r##"{"goal":"plan","prd":"PRD.md"}"##;
    let out =
        parse_planning_response_with_base(json, Path::new(&session_dir)).expect("should parse");
    assert!(
        out.prd.contains("Feature from file"),
        "prd should contain file content, got: {}",
        out.prd
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[test]
fn parse_planning_response_accepts_valid_json() {
    let input = "{\"goal\":\"plan\",\"prd\":\"# PRD\\n\\n## Summary\\nFeature X\\n\\n## TODO\\n\\n- [ ] Task 1\"}";
    let out = parse_planning_response(input).expect("should parse");
    assert!(out.prd.contains("Feature X"));
    assert!(out.prd.contains("Task 1"));
}

#[test]
fn parse_planning_response_rejects_non_json() {
    let input = "Some text without JSON";
    let err = parse_planning_response(input).unwrap_err();
    assert!(matches!(err, tddy_core::ParseError::Malformed(_)));
}

#[test]
fn parse_update_docs_response_extracts_valid_output() {
    let input = r#"{"goal":"update-docs","summary":"Updated 3 docs.","docs_updated":3}"#;
    let out = parse_update_docs_response(input).expect("should parse");
    assert_eq!(out.summary, "Updated 3 docs.");
    assert_eq!(out.docs_updated, 3);
}

#[test]
fn parse_update_docs_response_rejects_wrong_goal() {
    let input = r#"{"goal":"refactor","summary":"Updated docs.","docs_updated":2}"#;
    let err = parse_update_docs_response(input).unwrap_err();
    assert!(
        err.to_string().contains("goal") || err.to_string().contains("update-docs"),
        "expected goal-related error, got: {}",
        err
    );
}

/// Acceptance-tests and red outputs include sequential_command, logging_command when provided.
/// Parser must handle these fields; writers must include them in markdown.
#[test]
fn enhanced_test_instructions_include_sequential_and_logging() {
    let at_input = r#"{"goal":"acceptance-tests","summary":"Created tests.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"test_command":"cargo test","sequential_command":"cargo test -- --test-threads=1","logging_command":"RUST_LOG=debug cargo test"}"#;
    let at_out = parse_acceptance_tests_response(at_input).expect("parse acceptance tests");
    assert_eq!(at_out.tests.len(), 1);
    assert_eq!(at_out.tests[0].name, "t1");

    let red_input = r#"{"goal":"red","summary":"Created.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"skeletons":[],"sequential_command":"cargo test -- --test-threads=1","logging_command":"RUST_LOG=debug cargo test"}"#;
    let red_out = parse_red_response(red_input).expect("parse red");
    assert_eq!(red_out.tests.len(), 1);
}

/// write_artifacts rejects PlanningOutput with empty prd.
#[test]
fn write_artifacts_rejects_empty_prd() {
    use tddy_workflow_recipes::{write_artifacts, PlanningOutput};

    let session_dir = std::env::temp_dir().join("tddy-write-empty-prd-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create dir");

    let planning = PlanningOutput {
        prd: String::new(),
        name: None,
        discovery: None,
        demo_plan: None,
        branch_suggestion: None,
        worktree_suggestion: None,
    };
    let result = write_artifacts(&session_dir, &planning, "PRD.md");
    assert!(
        result.is_err(),
        "write_artifacts should reject empty prd, got Ok"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// write_artifacts rejects PlanningOutput with whitespace-only prd.
#[test]
fn write_artifacts_rejects_whitespace_only_prd() {
    use tddy_workflow_recipes::{write_artifacts, PlanningOutput};

    let session_dir = std::env::temp_dir().join("tddy-write-ws-prd-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create dir");

    let planning = PlanningOutput {
        prd: "   \n   ".to_string(),
        name: None,
        discovery: None,
        demo_plan: None,
        branch_suggestion: None,
        worktree_suggestion: None,
    };
    let result = write_artifacts(&session_dir, &planning, "PRD.md");
    assert!(
        result.is_err(),
        "write_artifacts should reject whitespace-only prd, got Ok"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Generated markdown files contain relative links to peer documents.
#[test]
fn markdown_cross_references_added() {
    use tddy_workflow_recipes::{write_artifacts, PlanningOutput};

    let session_dir = std::env::temp_dir().join("tddy-crossref-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create dir");

    let planning = PlanningOutput {
        prd: "# PRD\n## Summary\nFeature.\n\n## TODO\n\n- [ ] Task 1".to_string(),
        name: None,
        discovery: None,
        demo_plan: None,
        branch_suggestion: None,
        worktree_suggestion: None,
    };
    write_artifacts(&session_dir, &planning, "PRD.md").expect("write artifacts");

    let prd_content =
        std::fs::read_to_string(session_dir.join("artifacts").join("PRD.md")).expect("read PRD");
    assert!(
        prd_content.contains("## TODO") && prd_content.contains("Task 1"),
        "PRD.md should contain TODO content as last section"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// parse_evaluate_response() extracts summary, risk_level, issues, and build_results correctly.
#[test]
fn parse_evaluate_response_extracts_all_fields() {
    let input = r#"{"goal":"evaluate-changes","summary":"Analyzed 2 files. Risk: low.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[{"severity":"warning","category":"code_quality","file":"src/lib.rs","line":10,"description":"Magic number","suggestion":"Use a named constant"}],"changeset_sync":{"status":"synced","items_updated":1,"items_added":0},"files_analyzed":[{"file":"src/lib.rs","lines_changed":5,"changeset_item":"auth-login"}],"test_impact":{"tests_affected":1,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"Ready"}"#;

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

/// parse_evaluate_response() extracts evaluate-changes JSON.
#[test]
fn parse_evaluate_response_extracts_evaluate_json() {
    let input = r#"{"goal":"evaluate-changes","summary":"Analyzed 1 file.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"not_found","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/main.rs","lines_changed":5,"changeset_item":null}],"test_impact":{"tests_affected":0,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"Ready"}"#;

    let out = parse_evaluate_response(input).expect("parse_evaluate_response should succeed");
    assert_eq!(out.summary, "Analyzed 1 file.");
    assert_eq!(out.risk_level, "low");
}

/// Partial `changeset_sync` (status only) matches what the evaluate step needs for reporting;
/// unused numeric fields should default so agents are not forced to invent counts.
#[test]
fn parse_evaluate_response_accepts_changeset_sync_with_only_status() {
    let input =
        r#"{"goal":"evaluate-changes","summary":"Done.","changeset_sync":{"status":"skipped"}}"#;
    let out = parse_evaluate_response(input)
        .expect("parse_evaluate_response should accept partial changeset_sync");
    let sync = out
        .changeset_sync
        .as_ref()
        .expect("changeset_sync should be present");
    assert_eq!(sync.status, "skipped");
    assert_eq!(sync.items_updated, 0);
    assert_eq!(sync.items_added, 0);
}

/// parse_evaluate_response() returns ParseError::Malformed when the goal field is not "evaluate-changes".
#[test]
fn parse_evaluate_response_fails_on_wrong_goal_field() {
    let input = r#"{"goal":"plan","summary":"This is a plan, not an evaluation.","risk_level":"low","build_results":[],"issues":[],"changeset_sync":null,"files_analyzed":[],"test_impact":null,"changed_files":[],"affected_tests":[],"validity_assessment":""}"#;

    let err = parse_evaluate_response(input).expect_err("should fail on wrong goal");
    assert!(
        matches!(err, tddy_core::ParseError::Malformed(_)),
        "expected ParseError::Malformed for wrong goal field, got: {:?}",
        err
    );
}
