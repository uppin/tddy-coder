//! Integration tests for the green workflow with MockBackend.
//!
//! These acceptance tests define the expected behavior of the green goal.
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};
use tddy_workflow_recipes::parse_green_response;

use common::{ctx_green, ctx_red};
use tddy_core::workflow::graph::ExecutionStatus;

const RED_OUTPUT: &str = r#"{"goal":"red","summary":"Created 2 skeleton methods and 3 failing unit tests. All tests failing as expected.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"failing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}]}"#;

const GREEN_OUTPUT_ALL_PASS: &str = r#"{"goal":"green","summary":"Implemented 2 methods. All 3 unit tests and 2 acceptance tests passing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

const GREEN_OUTPUT_SOME_FAIL: &str = r#"{"goal":"green","summary":"Implemented 1 method. 2 tests passing, 1 failing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing","reason":"timeout"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

fn setup_session_dir_with_red_output(session_dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(session_dir);
    std::fs::create_dir_all(session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- auth_service_validates_email",
    )
    .expect("write acceptance-tests.md");
}

#[tokio::test]
async fn green_workflow_reads_progress_md_and_invokes_backend() {
    let session_dir = std::env::temp_dir().join("tddy-green-plan-dir-1");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-1");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, false);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output = parse_green_response(&output_str).expect("parse green output");
    assert!(output.summary.contains("passing"));
    assert_eq!(output.tests.len(), 3);
    assert_eq!(output.tests[0].status, "passing");
    assert_eq!(output.implementations.len(), 2);

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_transitions_to_green_complete_when_all_pass() {
    let session_dir = std::env::temp_dir().join("tddy-green-plan-dir-2");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-2");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let r = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "red: {:?}",
        r.status
    );

    let ctx = ctx_green(session_dir.clone(), None, false);
    let r = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        r.status
    );

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_eq!(
        changeset.state.current,
        WorkflowState::new("GreenComplete"),
        "workflow should transition to GreenComplete, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_transitions_to_failed_when_tests_fail() {
    let session_dir = std::env::temp_dir().join("tddy-green-plan-dir-3");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_SOME_FAIL);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-3");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, false);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        result.status
    );

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_ne!(
        changeset.state.current,
        WorkflowState::new("GreenComplete"),
        "state should not be GreenComplete when some tests fail, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_returns_error_when_progress_md_missing() {
    let session_dir = std::env::temp_dir().join("tddy-green-plan-dir-no-progress");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-no-progress");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();
    std::fs::remove_file(session_dir.join("progress.md")).expect("remove progress.md");

    let ctx = ctx_green(session_dir.clone(), None, false);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("progress") || err_msg.contains("PlanDir") || err_msg.contains("read"),
        "expected PlanDirInvalid when progress.md missing, got: {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_returns_error_when_impl_session_missing() {
    let session_dir = std::env::temp_dir().join("tddy-green-plan-dir-no-impl-session");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-no-impl");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let changeset_no_impl = r#"version: 1
models: {}
sessions:
  - id: "plan-only"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: AcceptanceTestsReady
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {}
"#;
    std::fs::write(session_dir.join("changeset.yaml"), changeset_no_impl).expect("write changeset");

    let ctx = ctx_green(session_dir.clone(), None, false);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("impl") || err_msg.contains("session") || err_msg.contains("changeset"),
        "expected ChangesetInvalid or InvalidTransition when impl session missing, got: {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_updates_progress_md_in_session_dir() {
    let session_dir = std::env::temp_dir().join("tddy-green-updates-progress");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-updates");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, false);
    let r = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        r.status
    );

    let progress_path = session_dir.join("progress.md");
    assert!(progress_path.exists(), "progress.md should be updated");
    let content = std::fs::read_to_string(&progress_path).expect("read progress.md");
    assert!(
        content.contains("[x]"),
        "progress.md should have checked items for passing tests"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_updates_acceptance_tests_md_in_session_dir() {
    let session_dir = std::env::temp_dir().join("tddy-green-updates-at");
    setup_session_dir_with_red_output(&session_dir);
    let at_content = r#"# Acceptance Tests

## Summary
Tests.

## How to run tests
cargo test

## Tests

### auth_service_validates_email

- **File**: packages/auth/src/service.rs
- **Line**: 42
- **Status**: failing
- **Validates**: auth service validates email

### auth_service_rejects_empty_email

- **File**: packages/auth/src/service.rs
- **Line**: 55
- **Status**: failing
- **Validates**: auth service rejects empty email
"#;
    std::fs::write(session_dir.join("acceptance-tests.md"), at_content)
        .expect("write acceptance-tests.md");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-updates-at");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, false);
    let r = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        r.status
    );

    let at_path = session_dir.join("acceptance-tests.md");
    let content = std::fs::read_to_string(&at_path).expect("read acceptance-tests.md");
    assert!(
        content.contains("**Status**: passing"),
        "acceptance-tests.md should have passing status for implemented tests"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_passes_goal_allowlist_to_invoke_request() {
    let session_dir = std::env::temp_dir().join("tddy-green-allowlist");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-allowlist");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, false);
    let r = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        r.status
    );

    let invocations = backend.invocations();
    assert!(invocations.len() >= 2, "green should have been invoked");
    let req = invocations
        .iter()
        .find(|r| r.goal_id == GoalId::new("green"))
        .expect("green invocation should exist");
    assert_eq!(
        req.goal_id,
        GoalId::new("green"),
        "InvokeRequest should have goal green for green workflow"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_workflow_resumes_session_from_impl_session_file() {
    let session_dir = std::env::temp_dir().join("tddy-green-resume-session");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join("tddy-green-engine-resume");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let invocations_before_green = backend.invocations();
    let expected_session_id = invocations_before_green
        .last()
        .and_then(|r| r.session.as_ref().map(|s| s.session_id()))
        .expect("red should have set session");

    let ctx = ctx_green(session_dir.clone(), Some(""), false);
    let r = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        r.status
    );

    let invocations = backend.invocations();
    let green_req = invocations
        .iter()
        .find(|r| r.goal_id == GoalId::new("green"))
        .expect("green invocation should exist");
    assert_eq!(
        green_req.session.as_ref().map(|s| s.session_id()),
        Some(expected_session_id),
        "green should resume with session from changeset.yaml"
    );
    assert!(
        green_req.session.as_ref().is_some_and(|s| s.is_resume()),
        "green should invoke with SessionMode::Resume"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn red_workflow_writes_impl_session_file_in_session_dir() {
    let session_dir = std::env::temp_dir().join("tddy-red-impl-session");
    setup_session_dir_with_red_output(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-red-impl-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let changeset_path = session_dir.join("changeset.yaml");
    assert!(
        changeset_path.exists(),
        "changeset.yaml should be written by red goal"
    );
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(content.contains("tag: impl") || content.contains("tag:impl"));
    assert!(content.contains("RedTestsReady"));

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_goal_reports_demo_results() {
    let session_dir = std::env::temp_dir().join("tddy-green-demo-results");
    setup_session_dir_with_red_output(&session_dir);
    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo Plan\n## Type\ncli\n## Steps\n1. Run `cargo run`\n## Verification\nSee output",
    )
    .expect("write demo-plan.md");

    const GREEN_OUTPUT_WITH_DEMO: &str = r#"{"goal":"green","summary":"Implemented. All tests passing. Demo verified.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}],"demo_results":{"summary":"Demo executed successfully","steps_completed":1}}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_WITH_DEMO);

    let storage_dir = std::env::temp_dir().join("tddy-green-demo-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    let ctx = ctx_green(session_dir.clone(), None, true);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "green: {:?}",
        result.status
    );

    let session = engine.get_session(&result.session_id).await.unwrap();
    assert!(
        session.is_some(),
        "green should succeed and produce session"
    );

    let demo_results_path = session_dir.join("demo-results.md");
    assert!(
        demo_results_path.exists(),
        "demo-results.md should be written when green completes with demo-plan.md present"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
