//! Integration tests for the new refactor goal.
//!
//! All tests are Red state — they fail to compile because the production
//! types and methods do not exist yet:
//! - `Goal::Refactor`
//! - `RefactorOptions`
//! - `WorkflowState::Refactoring` and `WorkflowState::RefactorComplete`
//! - `workflow.refactor()` method
//! - `RefactorOutput` struct
//! - `parse_refactor_response()` parser
//! - `refactor_allowlist()` permission function

use tddy_core::{
    BackendError, CodingBackend, CursorBackend, Goal, InvokeRequest, MockBackend, Workflow,
    WorkflowState,
};

/// Minimal refactor structured response for MockBackend.
const REFACTOR_OUTPUT: &str = r#"Refactoring complete. All tasks from refactoring-plan.md executed.

<structured-response content-type="application-json">
{"goal":"refactor","summary":"Executed 5 refactoring tasks. All tests passing after each change.","tasks_completed":5,"tests_passing":true}
</structured-response>
"#;

/// refactor() invokes backend with Goal::Refactor.
///
/// Fails to compile until Goal::Refactor exists and workflow.refactor() is implemented.
#[test]
fn refactor_invokes_backend_with_refactor_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-goal-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = tddy_core::RefactorOptions::default();
    let result = workflow.refactor(&plan_dir, None, &options);

    assert!(result.is_ok(), "refactor should succeed, got: {:?}", result);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::Refactor,
        "InvokeRequest must have goal Refactor"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() requires refactoring-plan.md in plan_dir.
///
/// Fails to compile until workflow.refactor() is implemented.
#[test]
fn refactor_requires_refactoring_plan() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-no-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    // Deliberately do NOT write refactoring-plan.md

    let backend = MockBackend::new();
    backend.push_ok(REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = tddy_core::RefactorOptions::default();
    let result = workflow.refactor(&plan_dir, None, &options);

    assert!(
        result.is_err(),
        "refactor should fail when refactoring-plan.md is missing"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() transitions workflow to RefactorComplete state on success.
///
/// Fails to compile until WorkflowState::RefactorComplete exists.
#[test]
fn refactor_transitions_to_refactor_complete() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-state-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = tddy_core::RefactorOptions::default();
    let _ = workflow.refactor(&plan_dir, None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::RefactorComplete { .. }),
        "workflow should transition to RefactorComplete, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() parses structured response with summary, tasks_completed, tests_passing.
///
/// Fails to compile until RefactorOutput and parse_refactor_response() exist.
#[test]
fn refactor_parses_structured_response() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-parse-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(REFACTOR_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = tddy_core::RefactorOptions::default();
    let result = workflow.refactor(&plan_dir, None, &options);

    assert!(result.is_ok(), "refactor should succeed, got: {:?}", result);
    let output = result.unwrap();

    assert!(!output.summary.is_empty(), "summary must not be empty");
    assert_eq!(output.tasks_completed, 5, "tasks_completed should be 5");
    assert!(output.tests_passing, "tests_passing should be true");

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// CursorBackend must reject Goal::Refactor with an "unsupported" error.
///
/// Fails to compile until Goal::Refactor exists and CursorBackend rejection is implemented.
#[test]
fn refactor_rejects_cursor_backend() {
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = InvokeRequest {
        prompt: "refactor".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Refactor,
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

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req));

    assert!(
        result.is_err(),
        "CursorBackend must return an error for Goal::Refactor"
    );
    match result {
        Err(BackendError::InvocationFailed(ref msg)) => {
            let msg_lower = msg.to_lowercase();
            assert!(
                msg_lower.contains("not supported")
                    || msg_lower.contains("cursor")
                    || msg_lower.contains("refactor"),
                "error message should indicate unsupported, got: {}",
                msg
            );
        }
        Err(BackendError::BinaryNotFound(_)) => {
            panic!(
                "CursorBackend must reject Goal::Refactor BEFORE spawning. \
                 Got BinaryNotFound — early rejection not implemented."
            );
        }
        #[allow(unreachable_patterns)]
        Err(e) => panic!("Expected InvocationFailed, got: {:?}", e),
        Ok(_) => panic!("Expected error, not Ok"),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn write_refactoring_plan(plan_dir: &std::path::Path) {
    let content = r#"# Refactoring Plan

## Priority: Critical

1. **Rename Goal::ValidateRefactor to Goal::Validate** (from: validate-tests-report, validate-prod-ready-report)
   - Scope: backend/mod.rs, workflow/mod.rs
   - Estimated effort: small

## Priority: High

2. **Rename internal types** (from: validate-tests-report, analyze-clean-code-report)
   - ValidateRefactorOptions → ValidateOptions
   - WorkflowState::ValidateRefactorComplete → ValidateComplete
   - Scope: workflow/mod.rs, lib.rs
   - Estimated effort: medium

## Priority: Medium

3. **Add refactoring-plan.md to KNOWN_ARTIFACTS** (from: analyze-clean-code-report)
   - Scope: workflow/mod.rs
   - Estimated effort: small

4. **Update validate schema** (from: validate-prod-ready-report)
   - Rename validate-refactor.schema.json → validate.schema.json
   - Estimated effort: small

5. **Add refactor goal types** (from: validate-tests-report)
   - Goal::Refactor, RefactorOptions, RefactorOutput
   - Estimated effort: medium
"#;
    std::fs::write(plan_dir.join("refactoring-plan.md"), content)
        .expect("write refactoring-plan.md");
}
