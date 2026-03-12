//! Integration tests for the validate workflow (subagent-based) with MockBackend and CursorBackend.
//!
//! Tests cover types and methods for the validate goal:
//! - `Goal::Validate`, `ValidateOptions`, `WorkflowState::ValidateComplete`
//! - `workflow.validate()` -> run_goal_until_done with ctx_validate
//! - `validate_subagents_allowlist()`
//!
//! CursorBackend must reject Goal::Validate immediately with an "unsupported" error,
//! before attempting to spawn the cursor process.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{
    validate_subagents_allowlist, BackendError, CodingBackend, CursorBackend, Goal, InvokeRequest,
    MockBackend, SharedBackend, WorkflowEngine,
};

use common::{
    ctx_validate, run_goal_until_done, write_changeset_with_state,
    write_evaluation_report_to_plan_dir,
};
use tddy_core::workflow::graph::ExecutionStatus;

/// Minimal validate (subagent) output as JSON (tddy-tools submit format).
const VALIDATE_REFACTOR_OUTPUT: &str = r#"{"goal":"validate","summary":"All 3 subagents completed. Reports written to plan-dir. Tests: 2 issues found. Production readiness: 1 blocker. Clean code score: 7/10.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

/// For run_goal_until_done(validate): validate -> refactor -> update-docs chain.
const REFACTOR_OUTPUT: &str = r#"{"goal":"refactor","summary":"Completed. All tests passing.","tasks_completed":5,"tests_passing":true}"#;

const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

/// validate() invokes backend with Goal::Validate.
#[tokio::test]
async fn validate_invokes_backend_with_validate_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-goal-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let result = engine.run_goal("validate", ctx).await;

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations
        .iter()
        .find(|r| r.goal == Goal::Validate)
        .expect("validate invocation");
    assert_eq!(
        req.goal,
        Goal::Validate,
        "InvokeRequest must have goal Validate"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() requires plan_dir — returns an error when plan_dir does not exist
/// or the working directory contains no evaluation-report.md.
#[tokio::test]
async fn validate_requires_plan_dir_with_evaluation_report() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-no-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    // Deliberately do NOT write evaluation-report.md — validate should fail

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-no-plan-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let result = run_goal_until_done(&engine, "validate", ctx).await;

    assert!(
        result.is_err(),
        "validate should fail when plan_dir has no evaluation-report.md — \
         validate depends on evaluation-report.md from a prior evaluate-changes run"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// CursorBackend must reject Goal::Validate with an "unsupported" error
/// before spawning the cursor process.
#[tokio::test]
async fn validate_rejects_cursor_backend() {
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = InvokeRequest {
        prompt: "validate refactor".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Validate,
        model: None,
        session: None,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
    };

    let result = backend.invoke(req).await;

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
#[tokio::test]
async fn validate_transitions_to_complete_state() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-state-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);
    write_changeset_with_state(&plan_dir, "Evaluated", "sess-eval-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let r = engine.run_goal("validate", ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        r.status
    );

    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert!(
        changeset.state.current == "ValidateComplete"
            || changeset.state.current == "RefactorComplete",
        "workflow should transition to ValidateComplete or RefactorComplete, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() correctly parses a structured response with tests/prod/clean-code flags.
#[tokio::test]
async fn validate_parses_structured_response() {
    let plan_dir = std::env::temp_dir().join("tddy-vr-parse-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-parse-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let result = engine.run_goal("validate", ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output =
        tddy_core::output::parse_validate_subagents_response(&output_str).expect("parse output");

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

/// validate() produces response with goal="validate".
#[tokio::test]
async fn validate_response_has_validate_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-validate-renamed-goal");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let validate_output_with_plan = r#"{"goal":"validate","summary":"All 3 subagents completed. Reports and refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(validate_output_with_plan);

    let storage_dir = std::env::temp_dir().join("tddy-validate-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let result = engine.run_goal("validate", ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output =
        tddy_core::output::parse_validate_subagents_response(&output_str).expect("parse output");

    assert_eq!(
        output.goal, "validate",
        "validate goal response should have goal='validate'"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate produces structured response with refactoring_plan_written field.
#[tokio::test]
async fn validate_produces_refactoring_plan() {
    let plan_dir = std::env::temp_dir().join("tddy-validate-refactoring-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);

    let validate_output = r#"{"goal":"validate","summary":"All 3 subagents completed. Refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(validate_output);

    let storage_dir = std::env::temp_dir().join("tddy-validate-refactor-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let result = engine.run_goal("validate", ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output =
        tddy_core::output::parse_validate_subagents_response(&output_str).expect("parse output");

    assert!(
        output.refactoring_plan_written,
        "structured response must include refactoring_plan_written: true"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate transitions to ValidateComplete state.
#[tokio::test]
async fn validate_transitions_to_validate_complete() {
    let plan_dir = std::env::temp_dir().join("tddy-validate-complete-state");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_evaluation_report_to_plan_dir(&plan_dir);
    write_changeset_with_state(&plan_dir, "Evaluated", "sess-eval-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-validate-complete-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_validate(plan_dir.clone());
    let _ = run_goal_until_done(&engine, "validate", ctx).await.unwrap();

    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert!(
        changeset.state.current == "ValidateComplete"
            || changeset.state.current == "RefactorComplete"
            || changeset.state.current == "DocsUpdated",
        "workflow should transition to ValidateComplete, RefactorComplete, or DocsUpdated, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}
