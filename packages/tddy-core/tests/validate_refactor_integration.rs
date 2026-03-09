//! Integration tests for the validate workflow (subagent-based) with MockBackend and CursorBackend.
//!
//! Tests cover types and methods for the validate goal:
//! - `Goal::Validate`, `ValidateOptions`, `WorkflowState::ValidateComplete`
//! - `workflow.validate()`, `validate_subagents_allowlist()`
//!
//! CursorBackend must reject Goal::Validate immediately with an "unsupported" error,
//! before attempting to spawn the cursor process.

use tddy_core::{
    validate_subagents_allowlist, BackendError, CodingBackend, CursorBackend, Goal, InvokeRequest,
    MockBackend, ValidateOptions, Workflow, WorkflowState,
};

/// Minimal validate (subagent) structured response.
const VALIDATE_REFACTOR_OUTPUT: &str = r#"All 3 subagents have completed their analysis.

validate-tests-report.md written.
validate-prod-ready-report.md written.
analyze-clean-code-report.md written.

<structured-response content-type="application-json">
{"goal":"validate","summary":"All 3 subagents completed. Reports written to plan-dir. Tests: 2 issues found. Production readiness: 1 blocker. Clean code score: 7/10.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true}
</structured-response>
"#;

/// validate() invokes backend with Goal::Validate.
#[test]
fn validate_invokes_backend_with_validate_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-goal-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::Validate,
        "InvokeRequest must have goal Validate"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() requires plan_dir — returns an error when plan_dir does not exist
/// or the working directory contains no evaluation-report.md.
#[test]
fn validate_requires_plan_dir_with_evaluation_report() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-no-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    // Deliberately do NOT write evaluation-report.md — validate should fail

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);

    assert!(
        result.is_err(),
        "validate should fail when plan_dir has no evaluation-report.md — \
         validate depends on evaluation-report.md from a prior evaluate-changes run"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// CursorBackend must reject Goal::Validate with an "unsupported" error
/// before spawning the cursor process.
///
/// The backend pointed at a nonexistent binary: if the early return works, we get
/// an InvocationFailed("not supported") error, NOT a BinaryNotFound error.
/// If early return is not implemented, the test fails (BinaryNotFound ≠ unsupported).
#[test]
fn validate_rejects_cursor_backend() {
    // Point at a nonexistent binary so any spawn attempt would produce BinaryNotFound.
    // The rejection must happen BEFORE spawning.
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = InvokeRequest {
        prompt: "validate refactor".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Validate,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let result = backend.invoke(req);

    assert!(
        result.is_err(),
        "CursorBackend must return an error for Goal::Validate"
    );

    match result {
        Err(BackendError::InvocationFailed(ref msg)) => {
            let msg_lower = msg.to_lowercase();
            assert!(
                msg_lower.contains("not supported")
                    || msg_lower.contains("cursor")
                    || msg_lower.contains("validate"),
                "error message should indicate the feature is unsupported on Cursor, got: {}",
                msg
            );
        }
        Err(BackendError::BinaryNotFound(_)) => {
            panic!(
                "CursorBackend must reject Goal::Validate BEFORE spawning the cursor process. \
                 Got BinaryNotFound, which means the early rejection is not implemented."
            );
        }
        #[allow(unreachable_patterns)]
        Err(e) => {
            panic!(
                "Expected InvocationFailed with unsupported message, got different error: {:?}",
                e
            );
        }
        Ok(_) => panic!("Expected error, CursorBackend must not accept Goal::Validate"),
    }
}

/// validate() transitions workflow to ValidateComplete state on success.
#[test]
fn validate_transitions_to_complete_state() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-state-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let _ = workflow.validate(&plan_dir, None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::ValidateComplete { .. }),
        "workflow should transition to ValidateComplete, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() correctly parses a structured response with tests/prod/clean-code flags.
#[test]
fn validate_parses_structured_response() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-parse-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);
    let output = result.unwrap();

    assert!(
        output.tests_report_written,
        "tests_report_written should be true, got: {:?}",
        output.tests_report_written
    );
    assert!(
        output.prod_ready_report_written,
        "prod_ready_report_written should be true, got: {:?}",
        output.prod_ready_report_written
    );
    assert!(
        output.clean_code_report_written,
        "clean_code_report_written should be true, got: {:?}",
        output.clean_code_report_written
    );
    assert!(
        !output.summary.is_empty(),
        "summary must not be empty, got: {:?}",
        output.summary
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate_subagents_allowlist() must include the Agent tool for spawning subagents.
#[test]
fn validate_subagents_allowlist_includes_agent_tool() {
    let allowlist = validate_subagents_allowlist();

    assert!(
        allowlist.iter().any(|t| t == "Agent"),
        "validate_subagents_allowlist must include Agent tool — \
         the orchestrator spawns 3 concurrent subagents via the Agent tool, got: {:?}",
        allowlist
    );

    // Also must retain the read-only analysis tools from evaluate_allowlist
    assert!(
        allowlist.iter().any(|t| t == "Read"),
        "validate_subagents_allowlist must include Read, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Glob"),
        "validate_subagents_allowlist must include Glob, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Write"),
        "validate_subagents_allowlist must include Write — subagents need to write their report MDs, got: {:?}",
        allowlist
    );
}

// ── validate goal acceptance tests ─────────────────────────────────────────────

/// validate() produces response with goal="validate".
#[test]
fn validate_response_has_validate_goal() {
    use tddy_core::{MockBackend, Workflow};

    let plan_dir = std::env::temp_dir().join("tddy-validate-renamed-goal");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let validate_output_with_plan = r#"All 3 subagents completed. Refactoring plan synthesized.

<structured-response content-type="application-json">
{"goal":"validate","summary":"All 3 subagents completed. Reports and refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}
</structured-response>
"#;

    let backend = MockBackend::new();
    backend.push_ok(validate_output_with_plan);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let _req = invocations.last().unwrap();

    let output = result.unwrap();
    assert_eq!(
        output.goal, "validate",
        "validate goal response should have goal='validate'"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate produces structured response with refactoring_plan_written field.
#[test]
fn validate_produces_refactoring_plan() {
    let plan_dir = std::env::temp_dir().join("tddy-validate-refactoring-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let validate_output = r#"All 3 subagents completed. Refactoring plan synthesized.

<structured-response content-type="application-json">
{"goal":"validate","summary":"All 3 subagents completed. Refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}
</structured-response>
"#;

    let backend = MockBackend::new();
    backend.push_ok(validate_output);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&plan_dir, None, &options);

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);
    let output = result.unwrap();

    assert!(
        output.refactoring_plan_written,
        "structured response must include refactoring_plan_written: true"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate transitions to ValidateComplete state.
#[test]
fn validate_transitions_to_validate_complete() {
    let plan_dir = std::env::temp_dir().join("tddy-validate-complete-state");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let _ = workflow.validate(&plan_dir, None, &options);

    let state = workflow.state();
    // Will fail until ValidateRefactorComplete is renamed to ValidateComplete
    assert!(
        matches!(state, WorkflowState::ValidateComplete { .. }),
        "workflow should transition to ValidateComplete (renamed from ValidateRefactorComplete), got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Write a minimal evaluation-report.md to plan_dir to satisfy validate's prerequisite.
fn write_evaluation_report_to_plan_dir(plan_dir: &std::path::Path) {
    let content = r#"# Evaluation Report

## Summary

Evaluated 3 changed files. Risk level: medium.

## Risk Level

medium

## Changed Files

- src/main.rs (modified, +15/-3)
- src/lib.rs (modified, +5/-0)
- tests/main_test.rs (added, +40/-0)

## Affected Tests

- tests/main_test.rs: created
- tests/integration_test.rs: updated

## Validity Assessment

The change is valid for the intended use-case.
"#;
    std::fs::write(plan_dir.join("evaluation-report.md"), content)
        .expect("write evaluation-report.md");
}
