//! Tests for Phase 1 renames (PRD R1, R2).
//!
//! These tests use the NEW type and method names specified by the PRD.
//! They will fail to compile until the renames are performed:
//!
//! - Goal::Validate → Goal::ValidateChanges (validate-changes goal)
//! - Goal::ValidateRefactor → Goal::Validate (subagent-based validate goal)
//! - ValidateOptions → ValidateChangesOptions
//! - ValidateRefactorOptions → ValidateOptions (subagent-based validate options)
//! - WorkflowState::Validated → WorkflowState::ValidateChangesComplete
//! - WorkflowState::Validating → WorkflowState::ValidatingChanges
//! - WorkflowState::ValidateRefactorComplete → WorkflowState::ValidateComplete
//! - ValidateRefactorOutput → ValidateSubagentsOutput
//! - workflow.validate() → workflow.validate_changes()
//! - workflow.validate_refactor() → workflow.validate()

use tddy_core::{
    Goal, MockBackend, ValidateChangesOptions, ValidateSubagentsOutput, Workflow, WorkflowState,
};

const VALIDATE_CHANGES_OUTPUT: &str = r#"Analyzed 5 changed files.

<structured-response content-type="application-json">
{"goal":"validate-changes","summary":"Analyzed 5 changed files. Low risk.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/lib.rs","lines_changed":10,"changeset_item":null}],"test_impact":{"tests_affected":2,"new_tests_needed":0}}
</structured-response>
"#;

const VALIDATE_SUBAGENTS_OUTPUT: &str = r#"All 3 subagents completed.

<structured-response content-type="application-json">
{"goal":"validate","summary":"All 3 subagents completed. Reports and refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}
</structured-response>
"#;

/// Goal::ValidateChanges should exist and be the validate-changes goal.
/// After Phase 1 rename: Goal::Validate → Goal::ValidateChanges.
#[test]
fn goal_validate_changes_variant_exists() {
    let goal = Goal::ValidateChanges;
    assert_eq!(
        format!("{:?}", goal),
        "ValidateChanges",
        "Goal::ValidateChanges should be the validate-changes goal"
    );
}

/// ValidateChangesOptions should exist as the options type for validate-changes.
/// After Phase 1 rename: ValidateOptions → ValidateChangesOptions.
#[test]
fn validate_changes_options_type_exists() {
    let options = ValidateChangesOptions::default();
    assert!(
        options.model.is_none(),
        "ValidateChangesOptions::default() model should be None"
    );
}

/// WorkflowState::ValidateChangesComplete should exist.
/// After Phase 1 rename: WorkflowState::Validated → WorkflowState::ValidateChangesComplete.
#[test]
fn workflow_state_validate_changes_complete_exists() {
    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_CHANGES_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let plan_dir = std::env::temp_dir().join("tddy-phase1-vc-complete");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let options = ValidateChangesOptions::default();
    let result = workflow.validate_changes(
        &std::path::Path::new("."),
        Some(plan_dir.as_path()),
        None,
        &options,
    );
    assert!(
        result.is_ok(),
        "validate_changes should succeed: {:?}",
        result
    );

    assert!(
        matches!(
            workflow.state(),
            WorkflowState::ValidateChangesComplete { .. }
        ),
        "after validate_changes, state should be ValidateChangesComplete, got {:?}",
        workflow.state()
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// WorkflowState::ValidatingChanges should exist.
/// After Phase 1 rename: WorkflowState::Validating → WorkflowState::ValidatingChanges.
#[test]
fn workflow_state_validating_changes_display_name() {
    let state = WorkflowState::ValidatingChanges;
    assert_eq!(
        state.display_name(),
        "ValidatingChanges",
        "ValidatingChanges display name should be 'ValidatingChanges'"
    );
}

/// WorkflowState::ValidateComplete should exist (renamed from ValidateRefactorComplete).
#[test]
fn workflow_state_validate_complete_exists() {
    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let plan_dir = std::env::temp_dir().join("tddy-phase1-v-complete");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("evaluation-report.md"),
        "# Evaluation Report\n## Summary\nAll good.",
    )
    .expect("write evaluation-report.md");

    let options = tddy_core::ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);
    assert!(
        result.is_ok(),
        "validate (subagent) should succeed: {:?}",
        result
    );

    assert!(
        matches!(workflow.state(), WorkflowState::ValidateComplete { .. }),
        "after validate (subagent), state should be ValidateComplete, got {:?}",
        workflow.state()
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// ValidateSubagentsOutput type should exist (renamed from ValidateRefactorOutput).
#[test]
fn validate_subagents_output_type_exists() {
    let output = ValidateSubagentsOutput {
        goal: "validate".to_string(),
        summary: "All 3 subagents completed.".to_string(),
        tests_report_written: true,
        prod_ready_report_written: true,
        clean_code_report_written: true,
        refactoring_plan_written: true,
    };
    assert!(output.tests_report_written);
    assert!(output.refactoring_plan_written);
}

/// workflow.validate_changes() should be the renamed validate-changes method.
/// After Phase 1: workflow.validate() → workflow.validate_changes().
#[test]
fn workflow_validate_changes_method_sends_validate_changes_goal() {
    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_CHANGES_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let plan_dir = std::env::temp_dir().join("tddy-phase1-vc-method");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let options = ValidateChangesOptions::default();
    let _result = workflow.validate_changes(
        &std::path::Path::new("."),
        Some(plan_dir.as_path()),
        None,
        &options,
    );

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty());
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::ValidateChanges,
        "validate_changes() should send Goal::ValidateChanges"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// workflow.validate() should now be the subagent-based validate method.
/// After Phase 1: workflow.validate_refactor() → workflow.validate().
/// The Goal should be Goal::Validate (the renamed subagent-based validate).
#[test]
fn workflow_validate_method_is_subagent_based() {
    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let plan_dir = std::env::temp_dir().join("tddy-phase1-v-subagent");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("evaluation-report.md"),
        "# Evaluation Report\n## Summary\nAll good.",
    )
    .expect("write evaluation-report.md");

    let options = tddy_core::ValidateOptions::default();
    let _result = workflow.validate(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty());
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::Validate,
        "validate() should now send Goal::Validate (the subagent-based validate)"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}
