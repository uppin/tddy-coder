//! Integration tests for the red workflow with MockBackend.

use tddy_core::{MockBackend, RedOptions, Workflow};

const RED_OUTPUT: &str = r#"Created skeleton code and failing tests.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created 2 skeleton methods and 3 failing unit tests. All tests failing as expected.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"failing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}]}
</structured-response>
"#;

#[test]
fn red_workflow_reads_prd_and_acceptance_tests_md_invokes_backend() {
    let plan_dir = std::env::temp_dir().join("tddy-red-plan-dir-1");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan\n- Test 1").expect("write PRD");
    std::fs::write(
        plan_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- login_stores_session_token",
    )
    .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let result = workflow.red(&plan_dir, None, &options);

    let output = result.expect("red should succeed");
    assert!(output.summary.contains("skeleton"));
    assert_eq!(output.tests.len(), 3);
    assert_eq!(output.tests[0].name, "auth_service_validates_email");
    assert_eq!(output.skeletons.len(), 2);
    assert_eq!(output.skeletons[0].name, "AuthService");
    assert_eq!(output.skeletons[0].kind, "struct");

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn red_workflow_transitions_through_red_testing_to_ready_states() {
    let plan_dir = std::env::temp_dir().join("tddy-red-plan-dir-2");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests")
        .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let _ = workflow.red(&plan_dir, None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, tddy_core::WorkflowState::RedTestsReady { .. }),
        "workflow should transition to RedTestsReady, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Red goal writes red-output.md to the plan directory after successful completion.
#[test]
fn red_workflow_writes_red_output_md_to_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-red-writes-md");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests")
        .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let _ = workflow.red(&plan_dir, None, &options);

    let md_path = plan_dir.join("red-output.md");
    assert!(
        md_path.exists(),
        "red-output.md should be written to plan directory, path: {}",
        md_path.display()
    );
    let content = std::fs::read_to_string(&md_path).expect("read red-output.md");
    assert!(
        content.contains("auth_service_validates_email"),
        "red-output.md should contain test names"
    );
    assert!(
        content.contains("AuthService"),
        "red-output.md should contain skeleton names"
    );
    assert!(
        content.contains("How to run tests"),
        "red-output.md should contain How to run tests section"
    );
    assert!(
        content.contains("Prerequisite actions"),
        "red-output.md should contain Prerequisite actions section"
    );
    assert!(
        content.contains("How to run a single or selected tests"),
        "red-output.md should contain How to run a single or selected tests section"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Red goal writes progress.md with unfilled checkboxes for failed tests and skeletons.
#[test]
fn red_workflow_writes_progress_md_to_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-red-progress-md");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests")
        .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let _ = workflow.red(&plan_dir, None, &options);

    let progress_path = plan_dir.join("progress.md");
    assert!(
        progress_path.exists(),
        "progress.md should be written to plan directory, path: {}",
        progress_path.display()
    );
    let content = std::fs::read_to_string(&progress_path).expect("read progress.md");
    assert!(
        content.contains("## Failed Tests"),
        "progress.md should contain Failed Tests section"
    );
    assert!(
        content.contains("## Skeletons"),
        "progress.md should contain Skeletons section"
    );
    assert!(
        content.contains("- [ ] auth_service_validates_email"),
        "progress.md should contain unfilled checkbox for test"
    );
    assert!(
        content.contains("- [ ] AuthService"),
        "progress.md should contain unfilled checkbox for skeleton"
    );
    assert!(
        content.contains("skipped") && content.contains("failed"),
        "progress.md should mention done/skipped/failed"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn red_workflow_returns_error_when_acceptance_tests_md_missing() {
    let plan_dir = std::env::temp_dir().join("tddy-red-plan-dir-no-at");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    // acceptance-tests.md is NOT created

    let backend = MockBackend::new();
    let mut workflow = Workflow::new(backend);

    let options = RedOptions::default();
    let result = workflow.red(&plan_dir, None, &options);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::PlanDirInvalid(_))),
        "expected PlanDirInvalid when acceptance-tests.md missing, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn red_workflow_passes_goal_allowlist_to_invoke_request() {
    let plan_dir = std::env::temp_dir().join("tddy-red-allowlist-test");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests")
        .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let _ = workflow.red(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        tddy_core::Goal::Red,
        "InvokeRequest should have goal Red for red workflow"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}
