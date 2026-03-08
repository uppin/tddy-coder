//! Integration tests for JSON Schema validation of structured agent output.

use std::path::Path;
use tddy_core::output::extract_last_structured_block;
use tddy_core::schema::{
    format_validation_errors, get_schema, schema_file_path, validate_output, write_schema_to_dir,
    SchemaError,
};

const VALID_GOALS: &[&str] = &[
    "plan",
    "acceptance-tests",
    "red",
    "green",
    "validate",
    "evaluate",
    "validate-refactor",
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
fn valid_validate_passes_schema_validation() {
    let json = include_str!("fixtures/valid/validate.json");
    assert!(validate_output("validate", json).is_ok());
}

#[test]
fn valid_evaluate_passes_schema_validation() {
    let json = include_str!("fixtures/valid/evaluate.json");
    assert!(validate_output("evaluate", json).is_ok());
}

#[test]
fn valid_validate_refactor_passes_schema_validation() {
    let json = include_str!("fixtures/valid/validate-refactor.json");
    assert!(validate_output("validate-refactor", json).is_ok());
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
    let err = validate_output("evaluate", json).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn invalid_acceptance_tests_missing_summary_fails() {
    let json = include_str!("fixtures/invalid/acceptance-tests-empty-summary.json");
    let err = validate_output("acceptance-tests", json).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn invalid_validate_refactor_wrong_goal_fails() {
    let json = include_str!("fixtures/invalid/validate-refactor-wrong-goal.json");
    let err = validate_output("validate-refactor", json).unwrap_err();
    assert!(!err.is_empty());
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

#[test]
fn schema_file_path_returns_relative_path() {
    assert_eq!(
        schema_file_path("plan"),
        Some("schemas/plan.schema.json".to_string())
    );
    assert_eq!(
        schema_file_path("red"),
        Some("schemas/red.schema.json".to_string())
    );
    assert!(schema_file_path("unknown").is_none());
}

#[test]
fn write_schema_to_dir_writes_files() {
    let tmp = std::env::temp_dir().join("tddy_schema_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let result = write_schema_to_dir(Path::new(&tmp), "plan");
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("plan.schema.json"));

    let schemas_dir = tmp.join("schemas");
    assert!(schemas_dir.exists());
    assert!(schemas_dir.join("plan.schema.json").exists());
    assert!(schemas_dir
        .join("common")
        .join("test-info.schema.json")
        .exists());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn extract_last_structured_block_extracts_schema_attribute() {
    let input = r#"<structured-response content-type="application-json" schema="schemas/red.schema.json">
{"goal":"red","summary":"x","tests":[]}
</structured-response>"#;
    let block = extract_last_structured_block(input).unwrap();
    assert_eq!(block.json, r#"{"goal":"red","summary":"x","tests":[]}"#);
    assert_eq!(block.schema, Some("schemas/red.schema.json"));
}

#[test]
fn extract_last_structured_block_handles_missing_schema_attribute() {
    let input = r#"<structured-response content-type="application-json">
{"goal":"red","summary":"x","tests":[]}
</structured-response>"#;
    let block = extract_last_structured_block(input).unwrap();
    assert_eq!(block.json, r#"{"goal":"red","summary":"x","tests":[]}"#);
    assert_eq!(block.schema, None);
}

#[test]
fn extract_last_structured_block_uses_last_block() {
    let input = r#"Example:
<structured-response schema="schemas/plan.schema.json">{"goal":"plan"}</structured-response>
Real output:
<structured-response schema="schemas/red.schema.json">{"goal":"red","summary":"ok","tests":[]}</structured-response>"#;
    let block = extract_last_structured_block(input).unwrap();
    assert!(block.json.contains("\"goal\":\"red\""));
    assert_eq!(block.schema, Some("schemas/red.schema.json"));
}
