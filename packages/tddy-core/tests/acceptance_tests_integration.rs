//! Integration tests for the acceptance-tests workflow with MockBackend.

use tddy_core::{AcceptanceTestsOptions, MockBackend, PlanOptions, Workflow};

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
    write_changeset_for_plan_session(&plan_dir, "sess-resume-123");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = AcceptanceTestsOptions::default();
    let result = workflow.acceptance_tests(&plan_dir, None, &options);

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
    write_changeset_for_plan_session(&plan_dir, "sess-456");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = AcceptanceTestsOptions::default();
    let _ = workflow.acceptance_tests(&plan_dir, None, &options);

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

    let options = AcceptanceTestsOptions::default();
    let result = workflow.acceptance_tests(&plan_dir, None, &options);

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

    let options = AcceptanceTestsOptions::default();
    let result = workflow.acceptance_tests(&plan_dir, None, &options);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::ChangesetMissing(_))),
        "expected ChangesetMissing, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Acceptance-tests goal passes goal-specific allowlist to InvokeRequest.
/// Allowlist should include Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch.
#[test]
fn acceptance_tests_workflow_passes_goal_allowlist_to_invoke_request() {
    let plan_dir = std::env::temp_dir().join("tddy-at-allowlist-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-allowlist");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = AcceptanceTestsOptions::default();
    let _ = workflow.acceptance_tests(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    let allowed = req
        .allowed_tools
        .as_ref()
        .expect("InvokeRequest should have allowed_tools set for acceptance-tests goal");
    let expected: Vec<&str> = vec![
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "Bash(cargo *)",
        "SemanticSearch",
    ];
    for tool in &expected {
        assert!(
            allowed.contains(&(*tool).to_string()),
            "allowlist should contain {}, got {:?}",
            tool,
            allowed
        );
    }

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Plan goal passes goal-specific allowlist to InvokeRequest.
/// Allowlist should include Read, Glob, Grep, SemanticSearch.
#[test]
fn plan_workflow_passes_goal_allowlist_to_invoke_request() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-plan-allowlist-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    let options = PlanOptions::default();
    let _ = workflow.plan("Build auth", &output_dir, None, &options);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    let allowed = req
        .allowed_tools
        .as_ref()
        .expect("InvokeRequest should have allowed_tools set for plan goal");
    let expected: Vec<&str> = vec!["Read", "Glob", "Grep", "SemanticSearch"];
    for tool in &expected {
        assert!(
            allowed.contains(&(*tool).to_string()),
            "allowlist should contain {}, got {:?}",
            tool,
            allowed
        );
    }

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Acceptance-tests goal writes acceptance-tests.md to the plan directory after successful completion.
#[test]
fn acceptance_tests_workflow_writes_acceptance_tests_md_to_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-at-writes-md");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-writes-md");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = AcceptanceTestsOptions::default();
    let _ = workflow.acceptance_tests(&plan_dir, None, &options);

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
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    assert!(changeset_path.exists(), "changeset.yaml should exist");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(content.contains("sessions:"));
    assert!(content.contains("plan"));
    assert!(content.contains("state:"));

    let _ = std::fs::remove_dir_all(&output_dir);
}

fn write_changeset_for_plan_session(plan_dir: &std::path::Path, session_id: &str) {
    let changeset = format!(
        r#"version: 1
models: {{}}
sessions:
  - id: "{}"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: Planned
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {{}}
"#,
        session_id
    );
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");
}
