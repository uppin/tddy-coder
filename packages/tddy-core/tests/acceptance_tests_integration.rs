//! Integration tests for the acceptance-tests workflow with MockBackend.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::output::parse_acceptance_tests_response;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

use common::{
    ctx_acceptance_tests, run_plan, temp_dir_with_git_repo, write_changeset_for_plan_session,
};

/// Plan output as JSON (tddy-tools submit format).
const PLAN_JSON_OUTPUT: &str = "{\"goal\":\"plan\",\"prd\":\"# Feature PRD\\n\\n## Summary\\nUser authentication system with login and logout.\\n\\n## Testing Plan\\n\\n### Test Level\\nIntegration - changes how auth component interacts with session storage.\\n\\n### Acceptance Tests\\n- [ ] **Integration**: Login stores session token (packages/auth/tests/session.it.rs)\\n- [ ] **Integration**: Logout clears session (packages/auth/tests/session.it.rs)\\n\\n## TODO\\n\\n- [ ] Create auth module\\n- [ ] Implement login endpoint\"}";

/// Acceptance-tests output as JSON (tddy-tools submit format).
const ACCEPTANCE_TESTS_JSON_OUTPUT: &str = r#"{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}"#;

#[tokio::test]
async fn acceptance_tests_workflow_reads_plan_dir_and_invokes_backend_with_resumed_session() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-plan-dir-1", "feature");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan\n- Test 1").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-resume-123");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-at-engine-1");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let result = engine.run_goal("acceptance-tests", context).await.unwrap();

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "acceptance-tests should succeed: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output = parse_acceptance_tests_response(&output_str).expect("parse output");
    assert!(output.summary.contains("Created 2 acceptance tests"));
    assert_eq!(output.tests.len(), 2);
    assert_eq!(output.tests[0].name, "login_stores_session_token");
    assert_eq!(output.tests[0].file, "packages/auth/tests/session.it.rs");
    assert_eq!(output.tests[0].status, "failing");

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn acceptance_tests_workflow_transitions_through_acceptance_testing_to_ready_states() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-plan-dir-2", "feature");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-456");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-at-engine-2");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let result = engine.run_goal("acceptance-tests", context).await.unwrap();

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "acceptance-tests should succeed: {:?}",
        result.status
    );

    let changeset = read_changeset(&plan_dir).expect("changeset should exist");
    assert_eq!(
        changeset.state.current, "AcceptanceTestsReady",
        "changeset state should be AcceptanceTestsReady"
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn acceptance_tests_workflow_returns_error_when_plan_dir_missing_prd() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-plan-dir-no-prd", "feature");
    std::fs::write(plan_dir.join(".session"), "sess-789").expect("write .session");

    let backend = Arc::new(MockBackend::new());
    let storage_dir = std::env::temp_dir().join("tddy-at-engine-no-prd");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let result = engine.run_goal("acceptance-tests", context).await;

    assert!(result.is_err(), "expected Error when PRD missing, got Ok");

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn acceptance_tests_workflow_returns_error_when_session_file_missing() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-plan-dir-no-session", "feature");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");

    let backend = Arc::new(MockBackend::new());
    let storage_dir = std::env::temp_dir().join("tddy-at-engine-no-session");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let result = engine.run_goal("acceptance-tests", context).await;

    assert!(
        result.is_err(),
        "expected Error when changeset missing, got Ok"
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn acceptance_tests_workflow_passes_goal_allowlist_to_invoke_request() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-allowlist-test", "feature");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-allowlist");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-at-engine-allowlist");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let _ = engine.run_goal("acceptance-tests", context).await.unwrap();

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        tddy_core::Goal::AcceptanceTests,
        "InvokeRequest should have goal AcceptanceTests"
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn plan_workflow_passes_goal_to_invoke_request() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-plan-allowlist-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-plan-engine-allowlist");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let _ = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .unwrap();

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        tddy_core::Goal::Plan,
        "InvokeRequest should have goal Plan"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[tokio::test]
async fn acceptance_tests_workflow_writes_acceptance_tests_md_to_plan_dir() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("at-writes-md", "feature");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-writes-md");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-at-engine-writes-md");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let context = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let _ = engine.run_goal("acceptance-tests", context).await.unwrap();

    let md_path = plan_dir.join("acceptance-tests.md");
    assert!(
        md_path.exists(),
        "acceptance-tests.md should be written to plan directory, path: {}",
        md_path.display()
    );
    let content = std::fs::read_to_string(&md_path).expect("read acceptance-tests.md");
    assert!(
        content.contains("login_stores_session_token"),
        "acceptance-tests.md should contain test names"
    );
    assert!(
        content.contains("failing"),
        "acceptance-tests.md should contain test status"
    );
    assert!(
        content.contains("How to run tests"),
        "acceptance-tests.md should contain How to run tests section"
    );
    assert!(
        content.contains("Prerequisite actions"),
        "acceptance-tests.md should contain Prerequisite actions section"
    );
    assert!(
        content.contains("How to run a single or selected tests"),
        "acceptance-tests.md should contain How to run a single or selected tests section"
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

#[tokio::test]
async fn plan_workflow_writes_session_file_to_output_directory() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-planning-session-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-session-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    assert!(changeset_path.exists(), "changeset.yaml should exist");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(content.contains("sessions:"));
    assert!(content.contains("plan"));
    assert!(content.contains("state:"));

    let _ = std::fs::remove_dir_all(&output_dir);
}
