//! Integration tests for the acceptance-tests workflow with MockBackend.

use tddy_core::{MockBackend, Workflow};

const DELIMITED_OUTPUT: &str = r#"Here is my analysis.

---PRD_START---
# Feature PRD

## Summary
User authentication system with login and logout.

## Testing Plan

### Test Level
Integration - changes how auth component interacts with session storage.

### Acceptance Tests
- [ ] **Integration**: Login stores session token (packages/auth/tests/session.it.rs)
- [ ] **Integration**: Logout clears session (packages/auth/tests/session.it.rs)
---PRD_END---

---TODO_START---
- [ ] Create auth module
- [ ] Implement login endpoint
---TODO_END---

That concludes the plan."#;

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"Created acceptance tests.

<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}
</structured-response>
"#;

#[test]
fn acceptance_tests_workflow_reads_plan_dir_and_invokes_backend_with_resumed_session() {
    let plan_dir = std::env::temp_dir().join("tddy-at-plan-dir-1");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan\n- Test 1").expect("write PRD");
    std::fs::write(plan_dir.join(".session"), "sess-resume-123").expect("write .session");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let result = workflow.acceptance_tests(&plan_dir, None, false, false, None);

    let output = result.expect("acceptance_tests should succeed");
    assert!(output.summary.contains("Created 2 acceptance tests"));
    assert_eq!(output.tests.len(), 2);
    assert_eq!(output.tests[0].name, "login_stores_session_token");
    assert_eq!(output.tests[0].file, "packages/auth/tests/session.it.rs");
    assert_eq!(output.tests[0].status, "failing");

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn acceptance_tests_workflow_transitions_through_acceptance_testing_to_ready_states() {
    let plan_dir = std::env::temp_dir().join("tddy-at-plan-dir-2");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join(".session"), "sess-456").expect("write .session");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let _ = workflow.acceptance_tests(&plan_dir, None, false, false, None);

    let state = workflow.state();
    assert!(
        matches!(state, tddy_core::WorkflowState::AcceptanceTestsReady { .. }),
        "workflow should transition to AcceptanceTestsReady, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn acceptance_tests_workflow_returns_error_when_plan_dir_missing_prd() {
    let plan_dir = std::env::temp_dir().join("tddy-at-plan-dir-no-prd");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join(".session"), "sess-789").expect("write .session");
    // PRD.md is NOT created

    let backend = MockBackend::new();
    let mut workflow = Workflow::new(backend);

    let result = workflow.acceptance_tests(&plan_dir, None, false, false, None);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::PlanDirInvalid(_))),
        "expected PlanDirInvalid, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn acceptance_tests_workflow_returns_error_when_session_file_missing() {
    let plan_dir = std::env::temp_dir().join("tddy-at-plan-dir-no-session");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    // .session is NOT created

    let backend = MockBackend::new();
    let mut workflow = Workflow::new(backend);

    let result = workflow.acceptance_tests(&plan_dir, None, false, false, None);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::SessionMissing(_))),
        "expected SessionMissing, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn plan_workflow_writes_session_file_to_output_directory() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-session-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, None, false, false)
        .expect("planning should succeed");

    let session_path = output_path.join(".session");
    assert!(session_path.exists(), ".session file should exist");
    let session_content = std::fs::read_to_string(&session_path).expect("read .session");
    assert!(!session_content.is_empty());
    assert!(
        session_content
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "session ID should be UUID-like"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
