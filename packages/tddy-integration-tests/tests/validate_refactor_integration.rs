//! Integration tests for the validate workflow (subagent-based) with MockBackend and CursorBackend.
//!
//! Tests cover types and methods for the validate goal:
//! - `Goal::Validate`, `ValidateOptions`, `WorkflowState::ValidateComplete`
//! - `workflow.validate()` -> run_goal_until_done with ctx_validate
//! - `validate_subagents_allowlist()`
//!
//! CursorBackend must invoke Goal::Validate using the same `agent` path as other goals
//! (parity with Claude), not reject it before spawn.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::{
    BackendError, CodingBackend, CursorBackend, GoalId, MockBackend, SharedBackend, WorkflowEngine,
};

use common::{
    ctx_validate, run_goal_until_done, write_changeset_with_state,
    write_evaluation_report_to_session_dir,
};
use serde_json::json;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::ids::WorkflowState;
use tddy_workflow_recipes::validate_subagents_allowlist;

/// Minimal validate (subagent) output as JSON (tddy-tools submit format).
const VALIDATE_REFACTOR_OUTPUT: &str = r#"{"goal":"validate","summary":"All 3 subagents completed. Reports written to plan-dir. Tests: 2 issues found. Production readiness: 1 blocker. Clean code score: 7/10.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

/// For run_goal_until_done(validate): validate -> refactor -> update-docs chain.
const REFACTOR_OUTPUT: &str = r#"{"goal":"refactor","summary":"Completed. All tests passing.","tasks_completed":5,"tests_passing":true}"#;

const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

/// validate() invokes backend with Goal::Validate.
#[tokio::test]
async fn validate_invokes_backend_with_validate_goal() {
    let session_dir = std::env::temp_dir().join("tddy-vr-goal-plan");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-vr-goal");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = engine.run_goal(&GoalId::new("validate"), ctx).await;

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations
        .iter()
        .find(|r| r.goal_id == GoalId::new("validate"))
        .expect("validate invocation");
    assert_eq!(
        req.goal_id,
        GoalId::new("validate"),
        "InvokeRequest must have goal_id validate"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// validate() requires session_dir — returns an error when session_dir does not exist
/// or the working directory contains no evaluation-report.md.
#[tokio::test]
async fn validate_requires_session_dir_with_evaluation_report() {
    let session_dir = std::env::temp_dir().join("tddy-vr-no-plan");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    // Deliberately do NOT write evaluation-report.md — validate should fail

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-no-plan-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = run_goal_until_done(&engine, "validate", ctx).await;

    assert!(
        result.is_err(),
        "validate should fail when session_dir has no evaluation-report.md — \
         validate depends on evaluation-report.md from a prior evaluate-changes run"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// CursorBackend must not return the legacy "validate is not supported on the Cursor backend"
/// error; Validate uses the same invocation path as other goals (Claude parity). A missing
/// binary yields `BinaryNotFound` after attempting spawn, not a pre-spawn rejection.
#[tokio::test]
async fn validate_cursor_backend_does_not_reject_goal_before_spawn() {
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = common::stub_invoke_request("validate refactor", "validate");

    let result = backend.invoke(req).await;

    assert!(
        !matches!(
            &result,
            Err(BackendError::InvocationFailed(ref m))
                if m == "validate is not supported on the Cursor backend"
        ),
        "Validate on Cursor must be implemented like Claude; got {:?}",
        result
    );
}

/// validate() transitions workflow to ValidateComplete state on success.
#[tokio::test]
async fn validate_transitions_to_complete_state() {
    let session_dir = std::env::temp_dir().join("tddy-vr-state-plan");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-eval-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let r = engine
        .run_goal(&GoalId::new("validate"), ctx)
        .await
        .unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        r.status
    );

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert!(
        changeset.state.current == WorkflowState::new("ValidateComplete")
            || changeset.state.current == WorkflowState::new("RefactorComplete"),
        "workflow should transition to ValidateComplete or RefactorComplete, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// validate() correctly parses a structured response with tests/prod/clean-code flags.
#[tokio::test]
async fn validate_parses_structured_response() {
    let session_dir = std::env::temp_dir().join("tddy-vr-parse-plan");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-vr-parse");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-vr-parse-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = engine
        .run_goal(&GoalId::new("validate"), ctx)
        .await
        .unwrap();
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
    let output = tddy_workflow_recipes::parse_validate_subagents_response(&output_str)
        .expect("parse output");

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

    let _ = std::fs::remove_dir_all(&session_dir);
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
    let session_dir = std::env::temp_dir().join("tddy-validate-renamed-goal");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-vr-renamed-goal");

    let validate_output_with_plan = r#"{"goal":"validate","summary":"All 3 subagents completed. Reports and refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(validate_output_with_plan);

    let storage_dir = std::env::temp_dir().join("tddy-validate-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = engine
        .run_goal(&GoalId::new("validate"), ctx)
        .await
        .unwrap();
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
    let output = tddy_workflow_recipes::parse_validate_subagents_response(&output_str)
        .expect("parse output");

    assert_eq!(
        output.goal, "validate",
        "validate goal response should have goal='validate'"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// validate produces structured response with refactoring_plan_written field.
#[tokio::test]
async fn validate_produces_refactoring_plan() {
    let session_dir = std::env::temp_dir().join("tddy-validate-refactoring-plan");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-vr-refactoring-plan");

    let validate_output = r#"{"goal":"validate","summary":"All 3 subagents completed. Refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(validate_output);

    let storage_dir = std::env::temp_dir().join("tddy-validate-refactor-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = engine
        .run_goal(&GoalId::new("validate"), ctx)
        .await
        .unwrap();
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
    let output = tddy_workflow_recipes::parse_validate_subagents_response(&output_str)
        .expect("parse output");

    assert!(
        output.refactoring_plan_written,
        "structured response must include refactoring_plan_written: true"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// After validate, `refactoring-plan.md` must contain the markdown from the `tddy-tools submit`
/// JSON (`refactoring_plan` field), not a generic placeholder, so the refactor step can read the
/// real plan.
#[tokio::test]
async fn validate_submit_refactoring_plan_field_written_to_refactoring_plan_md() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-vr-submit-refactor-plan-md-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-vr-submit-plan-md");

    const REFACTORING_PLAN_MARKDOWN: &str =
        "# Refactoring Plan\n\n## Tasks\n\n1. Remove `eprintln!` markers from production code.\n";

    let validate_json = json!({
        "goal": "validate",
        "summary": "Validation complete.",
        "tests_report_written": true,
        "prod_ready_report_written": true,
        "clean_code_report_written": true,
        "refactoring_plan_written": true,
        "refactoring_plan": REFACTORING_PLAN_MARKDOWN,
    })
    .to_string();

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(&validate_json);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-vr-submit-plan-md-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let result = engine
        .run_goal(&GoalId::new("validate"), ctx)
        .await
        .unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "validate: {:?}",
        result.status
    );

    let path = session_dir.join("refactoring-plan.md");
    assert!(
        path.exists(),
        "refactoring-plan.md must exist after validate when submit claims the plan was written"
    );
    let on_disk = std::fs::read_to_string(&path).expect("read refactoring-plan.md");
    assert_eq!(
        on_disk, REFACTORING_PLAN_MARKDOWN,
        "refactoring-plan.md must match the markdown from the validate tddy-tools submit JSON (refactoring_plan field)"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// validate transitions to ValidateComplete state.
#[tokio::test]
async fn validate_transitions_to_validate_complete() {
    let session_dir = std::env::temp_dir().join("tddy-validate-complete-state");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_evaluation_report_to_session_dir(&session_dir);
    write_changeset_with_state(&session_dir, "Evaluated", "sess-eval-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(VALIDATE_REFACTOR_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-validate-complete-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_validate(session_dir.clone());
    let _ = run_goal_until_done(&engine, "validate", ctx).await.unwrap();

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert!(
        changeset.state.current == WorkflowState::new("ValidateComplete")
            || changeset.state.current == WorkflowState::new("RefactorComplete")
            || changeset.state.current == WorkflowState::new("DocsUpdated"),
        "workflow should transition to ValidateComplete, RefactorComplete, or DocsUpdated, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
