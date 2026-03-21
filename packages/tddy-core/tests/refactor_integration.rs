//! Integration tests for the new refactor goal.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{
    BackendError, CodingBackend, CursorBackend, Goal, InvokeRequest, MockBackend, SharedBackend,
    WorkflowEngine,
};

use common::{
    ctx_refactor, run_goal_until_done, write_changeset_with_state, write_refactoring_plan,
};

/// Minimal refactor output as JSON (tddy-tools submit format).
const REFACTOR_OUTPUT: &str = r#"{"goal":"refactor","summary":"Executed 5 refactoring tasks. All tests passing after each change.","tasks_completed":5,"tests_passing":true}"#;

const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

/// refactor() invokes backend with Goal::Refactor.
#[tokio::test]
async fn refactor_invokes_backend_with_refactor_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-goal-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);
    write_changeset_with_state(&plan_dir, "ValidateComplete", "sess-refactor-goal");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-refactor-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_refactor(plan_dir.clone());
    let result = run_goal_until_done(&engine, "refactor", ctx).await;

    assert!(result.is_ok(), "refactor should succeed, got: {:?}", result);

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations
        .iter()
        .find(|r| r.goal == Goal::Refactor)
        .expect("refactor invocation should exist");
    assert_eq!(
        req.goal,
        Goal::Refactor,
        "InvokeRequest must have goal Refactor"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() requires refactoring-plan.md in plan_dir.
#[tokio::test]
async fn refactor_requires_refactoring_plan() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-no-plan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    // Deliberately do NOT write refactoring-plan.md

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-refactor-no-plan-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_refactor(plan_dir.clone());
    let result = run_goal_until_done(&engine, "refactor", ctx).await;

    assert!(
        result.is_err(),
        "refactor should fail when refactoring-plan.md is missing"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() transitions workflow to RefactorComplete state on success.
#[tokio::test]
async fn refactor_transitions_to_refactor_complete() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-state-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);
    write_changeset_with_state(&plan_dir, "ValidateComplete", "sess-validate-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-refactor-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_refactor(plan_dir.clone());
    let _ = run_goal_until_done(&engine, "refactor", ctx).await.unwrap();

    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert_eq!(
        changeset.state.current, "DocsUpdated",
        "workflow should transition to DocsUpdated (refactor -> update-docs), got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// refactor() parses structured response with summary, tasks_completed, tests_passing.
#[tokio::test]
async fn refactor_parses_structured_response() {
    let plan_dir = std::env::temp_dir().join("tddy-refactor-parse-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    write_refactoring_plan(&plan_dir);
    write_changeset_with_state(&plan_dir, "ValidateComplete", "sess-refactor-parse");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(REFACTOR_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-refactor-parse-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_refactor(plan_dir.clone());
    let result = engine.run_goal("refactor", ctx).await.unwrap();

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output = tddy_core::output::parse_refactor_response(&output_str).expect("parse output");

    assert!(!output.summary.is_empty(), "summary must not be empty");
    assert_eq!(output.tasks_completed, 5, "tasks_completed should be 5");
    assert!(output.tests_passing, "tests_passing should be true");

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// CursorBackend must not return the legacy "refactor is not supported on the Cursor backend"
/// error; Refactor uses the same invocation path as other goals (Claude parity). A missing
/// binary yields `BinaryNotFound` after attempting spawn, not a pre-spawn rejection.
#[tokio::test]
async fn refactor_cursor_backend_does_not_reject_goal_before_spawn() {
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/cursor"));
    let req = InvokeRequest {
        prompt: "refactor".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Refactor,
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
        plan_dir: None,
    };

    let result = backend.invoke(req).await;

    assert!(
        !matches!(
            &result,
            Err(BackendError::InvocationFailed(ref m))
                if m == "refactor is not supported on the Cursor backend"
        ),
        "Refactor on Cursor must be implemented like Claude; got {:?}",
        result
    );
}
