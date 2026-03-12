//! Integration tests for JSON Schema validation of structured agent output.

use std::path::Path;
use tddy_core::schema::{
    format_validation_errors, get_schema, schema_file_path, validate_output,
    write_all_schemas_to_dir, write_schema_to_dir, SchemaError,
};

const VALID_GOALS: &[&str] = &[
    "plan",
    "acceptance-tests",
    "red",
    "green",
    "evaluate",
    "validate-subagents",
    "refactor",
    "update-docs",
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
    assert!(validate_output("evaluate", json).is_ok());
}

#[test]
fn valid_validate_subagents_passes_schema_validation() {
    let json = include_str!("fixtures/valid/validate-subagents.json");
    assert!(validate_output("validate-subagents", json).is_ok());
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
fn invalid_validate_subagents_wrong_goal_fails() {
    let json = include_str!("fixtures/invalid/validate-subagents-wrong-goal.json");
    let err = validate_output("validate-subagents", json).unwrap_err();
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
fn write_all_schemas_to_dir_writes_all_goal_schemas_when_plan_dir_created() {
    let tmp = std::env::temp_dir().join("tddy_schema_all_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let result = write_all_schemas_to_dir(Path::new(&tmp));
    assert!(result.is_ok());

    let schemas_dir = tmp.join("schemas");
    let goals = [
        "plan.schema.json",
        "acceptance-tests.schema.json",
        "red.schema.json",
        "green.schema.json",
        "evaluate.schema.json",
        "validate-subagents.schema.json",
        "refactor.schema.json",
        "update-docs.schema.json",
    ];
    for f in &goals {
        assert!(schemas_dir.join(f).exists(), "{} should exist", f);
    }
    assert!(schemas_dir
        .join("common")
        .join("test-info.schema.json")
        .exists());

    let _ = std::fs::remove_dir_all(&tmp);
}
