//! Integration tests for JSON Schema validation of structured agent output.

use tddy_tools::schema::{format_validation_errors, get_schema, validate_output, SchemaError};

/// plan.schema.json must not contain the `plan_dir_suggestion` property.
///
/// Fails until `plan_dir_suggestion` is removed from the discovery object in the schema file.
#[test]
fn test_plan_dir_suggestion_removed_from_schema() {
    let content = get_schema("plan").expect("plan schema should exist");
    assert!(
        !content.contains("plan_dir_suggestion"),
        "plan.schema.json must not contain 'plan_dir_suggestion' property after R2 removal"
    );
}

const VALID_GOALS: &[&str] = &[
    "plan",
    "acceptance-tests",
    "red",
    "green",
    "evaluate-changes",
    "validate",
    "refactor",
    "update-docs",
    "demo",
];

#[test]
fn schema_files_are_embedded_and_retrievable() {
    for goal in VALID_GOALS {
        let content =
            get_schema(goal).unwrap_or_else(|| panic!("schema for {} should exist", goal));
        assert!(
            !content.is_empty(),
            "schema for {} should not be empty",
            goal
        );
        assert!(
            content.contains("$schema"),
            "schema for {} should contain $schema",
            goal
        );
    }
}

#[test]
fn valid_plan_passes_schema_validation() {
    let json = include_str!("fixtures/valid/plan.json");
    assert!(validate_output("plan", json).is_ok());
}

#[test]
fn valid_acceptance_tests_passes_schema_validation() {
    let json = include_str!("fixtures/valid/acceptance-tests.json");
    assert!(validate_output("acceptance-tests", json).is_ok());
}

#[test]
fn valid_red_passes_schema_validation() {
    let json = include_str!("fixtures/valid/red.json");
    assert!(validate_output("red", json).is_ok());
}

#[test]
fn valid_green_passes_schema_validation() {
    let json = include_str!("fixtures/valid/green.json");
    assert!(validate_output("green", json).is_ok());
}

#[test]
fn valid_evaluate_passes_schema_validation() {
    let json = include_str!("fixtures/valid/evaluate.json");
    assert!(validate_output("evaluate-changes", json).is_ok());
}

#[test]
fn valid_validate_subagents_passes_schema_validation() {
    let json = include_str!("fixtures/valid/validate-subagents.json");
    assert!(validate_output("validate", json).is_ok());
}

#[test]
fn valid_refactor_passes_schema_validation() {
    let json = r#"{"goal":"refactor","summary":"Completed 3 tasks.","tasks_completed":3,"tests_passing":true}"#;
    assert!(validate_output("refactor", json).is_ok());
}

#[test]
fn valid_update_docs_passes_schema_validation() {
    let json = r#"{"goal":"update-docs","summary":"Updated 3 docs.","docs_updated":3}"#;
    assert!(validate_output("update-docs", json).is_ok());
}

#[test]
fn valid_demo_passes_schema_validation() {
    let json = r#"{"goal":"demo","summary":"Demo completed.","demo_type":"cli","steps_completed":2,"verification":"All steps passed."}"#;
    assert!(validate_output("demo", json).is_ok());
}

#[test]
fn invalid_update_docs_wrong_goal_fails() {
    let json = r#"{"goal":"refactor","summary":"Updated docs.","docs_updated":2}"#;
    let err = validate_output("update-docs", json).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn invalid_plan_missing_prd_fails_with_descriptive_errors() {
    let json = include_str!("fixtures/invalid/plan-missing-prd.json");
    let err = validate_output("plan", json).unwrap_err();
    assert!(!err.is_empty());
    assert!(
        err.iter()
            .any(|e| e.message.contains("prd") || e.instance_path.contains("prd")),
        "errors should mention prd: {:?}",
        err
    );
}

#[test]
fn red_output_with_markers_passes_schema_validation() {
    let json = r#"{"goal":"red","summary":"Created skeletons and failing tests with logging markers.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"markers":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","data":{"user":"test@example.com"}}],"marker_results":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","collected":true,"investigation":null}]}"#;
    assert!(validate_output("red", json).is_ok());
}

#[test]
fn invalid_red_wrong_test_type_fails_with_field_path() {
    let json = include_str!("fixtures/invalid/red-wrong-test-type.json");
    let err = validate_output("red", json).unwrap_err();
    assert!(!err.is_empty());
    assert!(
        err.iter()
            .any(|e| e.instance_path.contains("tests") || e.message.contains("integer")),
        "errors should mention tests or type: {:?}",
        err
    );
}

#[test]
fn invalid_green_missing_summary_fails() {
    let json = include_str!("fixtures/invalid/green-missing-summary.json");
    let err = validate_output("green", json).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn invalid_evaluate_missing_goal_fails() {
    let json = include_str!("fixtures/invalid/evaluate-missing-goal.json");
    let err = validate_output("evaluate-changes", json).unwrap_err();
    assert!(!err.is_empty());
}

/// Evaluate output should not require changeset sync counters when the workflow only needs a
/// successful submit and report generation; those fields are not used for state transitions.
#[test]
fn evaluate_schema_accepts_partial_changeset_sync_status_only() {
    let json =
        r#"{"goal":"evaluate-changes","summary":"Done.","changeset_sync":{"status":"skipped"}}"#;
    assert!(
        validate_output("evaluate-changes", json).is_ok(),
        "evaluate-changes schema should accept changeset_sync with only status; workflow does not require items_updated/items_added for hooks"
    );
}

#[test]
fn invalid_evaluate_build_results_use_name_instead_of_package_fails() {
    let json = r#"{"goal":"evaluate-changes","summary":"x","risk_level":"low","build_results":[{"name":"tddy-core","status":"pass"}],"issues":[],"changed_files":[],"affected_tests":[],"validity_assessment":"x"}"#;
    let err = validate_output("evaluate-changes", json).unwrap_err();
    assert!(!err.is_empty());
    assert!(
        err.iter().any(|e| {
            e.instance_path.contains("build_results")
                && (e.message.contains("package") || e.schema_path.contains("package"))
        }),
        "errors should reference required package on build_results items: {:?}",
        err
    );
}

#[test]
fn invalid_acceptance_tests_missing_summary_fails() {
    let json = include_str!("fixtures/invalid/acceptance-tests-empty-summary.json");
    let err = validate_output("acceptance-tests", json).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn invalid_validate_subagents_wrong_goal_fails() {
    let json = include_str!("fixtures/invalid/validate-subagents-wrong-goal.json");
    let err = validate_output("validate", json).unwrap_err();
    assert!(!err.is_empty());
}

/// `red.schema.json` must stay identical between tddy-core and tddy-tools, and document
/// `source_file` on markers for placement validation (PRD acceptance).
#[test]
fn red_schema_parity_core_and_tools() {
    const CORE: &str = include_str!("../../tddy-core/schemas/red.schema.json");
    const TOOLS: &str = include_str!("../schemas/red.schema.json");
    assert_eq!(
        CORE, TOOLS,
        "packages/tddy-core/schemas/red.schema.json must match packages/tddy-tools/schemas/red.schema.json"
    );
    assert!(
        CORE.contains("\"source_file\""),
        "red.schema.json markers items must include source_file for production-only validation"
    );
}

#[test]
fn format_validation_errors_produces_readable_output() {
    let errors = vec![
        SchemaError {
            instance_path: "/summary".to_string(),
            schema_path: "/properties/summary".to_string(),
            message: "\"summary\" is a required property".to_string(),
        },
        SchemaError {
            instance_path: "/tests/0/line".to_string(),
            schema_path: "/properties/tests/items/properties/line/type".to_string(),
            message: "\"ten\" is not of type \"integer\"".to_string(),
        },
    ];
    let formatted = format_validation_errors(&errors);
    assert!(formatted.contains("/summary"));
    assert!(formatted.contains("required property"));
    assert!(formatted.contains("/tests/0/line"));
    assert!(formatted.contains("integer"));
}
