//! Integration tests for the validate-refactor workflow with MockBackend and CursorBackend.
//!
//! All tests reference types and methods introduced by the validate-refactor goal:
//! - `Goal::ValidateRefactor` (new variant)
//! - `ValidateRefactorOptions` (new struct)
//! - `WorkflowState::ValidateRefactorComplete` (new state)
//! - `workflow.validate_refactor()` method (new method)
//! - `validate_refactor_allowlist()` (new allowlist including Agent tool)
//!
//! Additional test: CursorBackend must reject Goal::ValidateRefactor immediately
//! with an "unsupported" error, before attempting to spawn the cursor process.
//!
//! These tests are in Red state — they fail to compile because the production
//! code has not been implemented yet.

use tddy_core::{
    validate_refactor_allowlist, BackendError, CodingBackend, CursorBackend, Goal, InvokeRequest,
    MockBackend, ValidateRefactorOptions, Workflow, WorkflowState,
};

/// Minimal validate-refactor structured response.
const VALIDATE_REFACTOR_OUTPUT: &str = r#"All 3 subagents have completed their analysis.

validate-tests-report.md written.
validate-prod-ready-report.md written.
analyze-clean-code-report.md written.

<structured-response content-type="application-json">
{"goal":"validate-refactor","summary":"All 3 subagents completed. Reports written to plan-dir. Tests: 2 issues found. Production readiness: 1 blocker. Clean code score: 7/10.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true}
</structured-response>
"#;

/// validate_refactor() invokes backend with Goal::ValidateRefactor.
#[test]
fn validate_refactor_invokes_backend_with_validate_refactor_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-goal-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateRefactorOptions::default();
    let result = workflow.validate_refactor(&plan_dir, None, &options);

    assert!(
        result.is_ok(),
        "validate_refactor should succeed, got: {:?}",
        result
    );

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::ValidateRefactor,
        "InvokeRequest must have goal ValidateRefactor"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate_refactor() requires plan_dir — returns an error when plan_dir does not exist
/// or the working directory contains no evaluation-report.md.
#[test]
fn validate_refactor_requires_plan_dir_with_evaluation_report() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-no-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    // Deliberately do NOT write evaluation-report.md — validate-refactor should fail

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateRefactorOptions::default();
    let result = workflow.validate_refactor(&plan_dir, None, &options);

    assert!(
        result.is_err(),
        "validate_refactor should fail when plan_dir has no evaluation-report.md — \
         validate-refactor depends on evaluation-report.md from a prior evaluate-changes run"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// CursorBackend must reject Goal::ValidateRefactor with an "unsupported" error
/// before spawning the cursor process.
///
/// The backend pointed at a nonexistent binary: if the early return works, we get
/// an InvocationFailed("not supported") error, NOT a BinaryNotFound error.
/// If early return is not implemented, the test fails (BinaryNotFound ≠ unsupported).
#[test]
fn validate_refactor_rejects_cursor_backend() {
    // Point at a nonexistent binary so any spawn attempt would produce BinaryNotFound.
    // The rejection must happen BEFORE spawning.
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = InvokeRequest {
        prompt: "validate refactor".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::ValidateRefactor,
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
        "CursorBackend must return an error for Goal::ValidateRefactor"
    );

    match result {
        Err(BackendError::InvocationFailed(ref msg)) => {
            let msg_lower = msg.to_lowercase();
            assert!(
                msg_lower.contains("not supported")
                    || msg_lower.contains("cursor")
                    || msg_lower.contains("validate-refactor"),
                "error message should indicate the feature is unsupported on Cursor, got: {}",
                msg
            );
        }
        Err(BackendError::BinaryNotFound(_)) => {
            panic!(
                "CursorBackend must reject Goal::ValidateRefactor BEFORE spawning the cursor process. \
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
        Ok(_) => panic!("Expected error, CursorBackend must not accept Goal::ValidateRefactor"),
    }
}

/// validate_refactor() transitions workflow to ValidateRefactorComplete state on success.
#[test]
fn validate_refactor_transitions_to_complete_state() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-state-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateRefactorOptions::default();
    let _ = workflow.validate_refactor(&plan_dir, None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::ValidateRefactorComplete { .. }),
        "workflow should transition to ValidateRefactorComplete, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate_refactor() correctly parses a structured response with tests/prod/clean-code flags.
#[test]
fn validate_refactor_parses_structured_response() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-parse-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateRefactorOptions::default();
    let result = workflow.validate_refactor(&plan_dir, None, &options);

    assert!(
        result.is_ok(),
        "validate_refactor should succeed, got: {:?}",
        result
    );
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

/// validate_refactor_allowlist() must include the Agent tool for spawning subagents.
#[test]
fn validate_refactor_allowlist_includes_agent_tool() {
    let allowlist = validate_refactor_allowlist();

    assert!(
        allowlist.iter().any(|t| t == "Agent"),
        "validate_refactor_allowlist must include Agent tool — \
         the orchestrator spawns 3 concurrent subagents via the Agent tool, got: {:?}",
        allowlist
    );

    // Also must retain the read-only analysis tools from evaluate_allowlist
    assert!(
        allowlist.iter().any(|t| t == "Read"),
        "validate_refactor_allowlist must include Read, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Glob"),
        "validate_refactor_allowlist must include Glob, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Write"),
        "validate_refactor_allowlist must include Write — subagents need to write their report MDs, got: {:?}",
        allowlist
    );
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Write a minimal evaluation-report.md to plan_dir to satisfy validate_refactor's prerequisite.
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
