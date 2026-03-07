//! Integration tests for the green workflow with MockBackend.
//!
//! These acceptance tests define the expected behavior of the green goal.
//! They fail until the green workflow is implemented.

use tddy_core::{MockBackend, RedOptions, Workflow};

const RED_OUTPUT: &str = r#"Created skeleton code and failing tests.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created 2 skeleton methods and 3 failing unit tests. All tests failing as expected.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"failing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}]}
</structured-response>
"#;

const GREEN_OUTPUT_ALL_PASS: &str = r#"Implemented production code. All tests passing.

<structured-response content-type="application-json">
{"goal":"green","summary":"Implemented 2 methods. All 3 unit tests and 2 acceptance tests passing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;

const GREEN_OUTPUT_SOME_FAIL: &str = r#"Implemented partial. Some tests still failing.

<structured-response content-type="application-json">
{"goal":"green","summary":"Implemented 1 method. 2 tests passing, 1 failing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing","reason":"timeout"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;

fn setup_plan_dir_with_red_output(plan_dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(plan_dir);
    std::fs::create_dir_all(plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        plan_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- auth_service_validates_email",
    )
    .expect("write acceptance-tests.md");
}

#[test]
fn green_workflow_reads_progress_md_and_invokes_backend() {
    let plan_dir = std::env::temp_dir().join("tddy-green-plan-dir-1");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let result = workflow.green(&plan_dir, None, &options);

    let output = result.expect("green should succeed");
    assert!(output.summary.contains("passing"));
    assert_eq!(output.tests.len(), 3);
    assert_eq!(output.tests[0].status, "passing");
    assert_eq!(output.implementations.len(), 2);

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_transitions_to_green_complete_when_all_pass() {
    let plan_dir = std::env::temp_dir().join("tddy-green-plan-dir-2");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let _ = workflow.green(&plan_dir, None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, tddy_core::WorkflowState::GreenComplete { .. }),
        "workflow should transition to GreenComplete, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_transitions_to_failed_when_tests_fail() {
    let plan_dir = std::env::temp_dir().join("tddy-green-plan-dir-3");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_SOME_FAIL);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let result = workflow.green(&plan_dir, None, &options);

    assert!(result.is_err(), "green should return Err when tests fail");
    assert!(
        matches!(workflow.state(), tddy_core::WorkflowState::Failed { .. }),
        "workflow should transition to Failed when tests fail, got {:?}",
        workflow.state()
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_returns_error_when_progress_md_missing() {
    let plan_dir = std::env::temp_dir().join("tddy-green-plan-dir-no-progress");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    std::fs::remove_file(plan_dir.join("progress.md")).expect("remove progress.md");

    // Green fails before invoking backend (progress.md missing), so no second response needed
    let options = tddy_core::GreenOptions::default();
    let result = workflow.green(&plan_dir, None, &options);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::PlanDirInvalid(_))),
        "expected PlanDirInvalid when progress.md missing, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_returns_error_when_impl_session_missing() {
    let plan_dir = std::env::temp_dir().join("tddy-green-plan-dir-no-impl-session");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    // Overwrite changeset with one that has no impl session — green should fail
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
    std::fs::write(plan_dir.join("changeset.yaml"), changeset_no_impl).expect("write changeset");

    let options = tddy_core::GreenOptions::default();
    let result = workflow.green(&plan_dir, None, &options);

    assert!(result.is_err());
    assert!(
        matches!(
            result,
            Err(tddy_core::WorkflowError::ChangesetInvalid(_))
                | Err(tddy_core::WorkflowError::InvalidTransition(_))
        ),
        "expected ChangesetInvalid or InvalidTransition when impl session missing, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_updates_progress_md_in_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-green-updates-progress");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let _ = workflow.green(&plan_dir, None, &options);

    let progress_path = plan_dir.join("progress.md");
    assert!(progress_path.exists(), "progress.md should be updated");
    let content = std::fs::read_to_string(&progress_path).expect("read progress.md");
    assert!(
        content.contains("[x]"),
        "progress.md should have checked items for passing tests"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_updates_acceptance_tests_md_in_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-green-updates-at");
    setup_plan_dir_with_red_output(&plan_dir);
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
    std::fs::write(plan_dir.join("acceptance-tests.md"), at_content)
        .expect("write acceptance-tests.md");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let _ = workflow.green(&plan_dir, None, &options);

    let at_path = plan_dir.join("acceptance-tests.md");
    let content = std::fs::read_to_string(&at_path).expect("read acceptance-tests.md");
    assert!(
        content.contains("**Status**: passing"),
        "acceptance-tests.md should have passing status for implemented tests"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_passes_goal_allowlist_to_invoke_request() {
    let plan_dir = std::env::temp_dir().join("tddy-green-allowlist");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let _ = workflow.green(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    assert!(invocations.len() >= 2, "green should have been invoked");
    let req = invocations.last().unwrap();
    let allowed = req.allowed_tools.as_ref().expect("allowed_tools set");
    let expected = vec![
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
            "allowlist should contain {}",
            tool
        );
    }

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_workflow_resumes_session_from_impl_session_file() {
    let plan_dir = std::env::temp_dir().join("tddy-green-resume-session");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());

    let invocations_before_green = workflow.backend().invocations();
    let expected_session_id = invocations_before_green
        .last()
        .and_then(|r| r.session_id.as_deref())
        .expect("red should have set session_id");

    let options = tddy_core::GreenOptions::default();
    let _ = workflow.green(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    let green_req = invocations.last().unwrap();
    assert_eq!(
        green_req.session_id.as_deref(),
        Some(expected_session_id),
        "green should resume with session_id from changeset.yaml"
    );
    assert!(
        green_req.is_resume,
        "green should invoke with is_resume=true"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn red_workflow_writes_impl_session_to_plan_dir() {
    let plan_dir = std::env::temp_dir().join("tddy-red-impl-session");
    setup_plan_dir_with_red_output(&plan_dir);

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());

    let changeset_path = plan_dir.join("changeset.yaml");
    assert!(
        changeset_path.exists(),
        "changeset.yaml should be written by red goal"
    );
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(content.contains("tag: impl") || content.contains("tag:impl"));
    assert!(content.contains("RedTestsReady"));

    let _ = std::fs::remove_dir_all(&plan_dir);
}

#[test]
fn green_goal_reports_demo_results() {
    let plan_dir = std::env::temp_dir().join("tddy-green-demo-results");
    setup_plan_dir_with_red_output(&plan_dir);
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo Plan\n## Type\ncli\n## Steps\n1. Run `cargo run`\n## Verification\nSee output",
    )
    .expect("write demo-plan.md");

    const GREEN_OUTPUT_WITH_DEMO: &str = r#"Implemented. Demo verified.

<structured-response content-type="application-json">
{"goal":"green","summary":"Implemented. All tests passing. Demo verified.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}],"demo_results":{"summary":"Demo executed successfully","steps_completed":1}}
</structured-response>
"#;

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_WITH_DEMO);
    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let options = tddy_core::GreenOptions::default();
    let result = workflow.green(&plan_dir, None, &options);

    let _output = result.expect("green should succeed");

    let demo_results_path = plan_dir.join("demo-results.md");
    assert!(
        demo_results_path.exists(),
        "demo-results.md should be written when green completes with demo-plan.md present"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}
