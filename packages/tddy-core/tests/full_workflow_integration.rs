//! Integration tests for the full workflow (plan -> acceptance-tests -> red -> green).
//!
//! These acceptance tests define the expected behavior when running the full workflow
//! without a specific --goal. They verify chaining, resume logic, and next_goal_for_state.

use tddy_core::{
    next_goal_for_state, AcceptanceTestsOptions, MockBackend, PlanOptions, RedOptions, Workflow,
};
use tddy_core::{GreenOptions, WorkflowState};

const PLAN_OUTPUT: &str = r#"Here is my analysis.

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

fn write_changeset_with_state(plan_dir: &std::path::Path, state: &str, session_id: &str) {
    let changeset = format!(
        r#"version: 1
models: {{}}
sessions:
  - id: "{}"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: {}
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {{}}
"#,
        session_id, state
    );
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");
}

/// Full workflow chains plan -> acceptance-tests -> red -> green on a single Workflow.
#[test]
fn full_workflow_chains_all_steps() {
    let output_dir = std::env::temp_dir().join("tddy-full-workflow-chain");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let mut workflow = Workflow::new(backend);
    let plan_options = PlanOptions::default();
    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &plan_options)
        .expect("plan should succeed");

    let at_options = AcceptanceTestsOptions::default();
    let _ = workflow
        .acceptance_tests(&plan_dir, None, &at_options)
        .expect("acceptance_tests should succeed");

    let red_options = RedOptions::default();
    let _ = workflow
        .red(&plan_dir, None, &red_options)
        .expect("red should succeed");

    let green_options = GreenOptions::default();
    let green_output = workflow
        .green(&plan_dir, None, &green_options)
        .expect("green should succeed");

    assert!(green_output.summary.contains("passing"));
    assert_eq!(green_output.tests.len(), 3);
    assert_eq!(green_output.implementations.len(), 2);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::GreenComplete { .. }),
        "workflow should end in GreenComplete, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// next_goal_for_state maps each workflow state to the next goal.
#[test]
fn next_goal_for_state_maps_states_correctly() {
    assert_eq!(next_goal_for_state("Init"), Some("plan"));
    assert_eq!(next_goal_for_state("Planned"), Some("acceptance-tests"));
    assert_eq!(next_goal_for_state("AcceptanceTestsReady"), Some("red"));
    assert_eq!(next_goal_for_state("RedTestsReady"), Some("green"));
    assert_eq!(next_goal_for_state("GreenComplete"), None);
    assert_eq!(next_goal_for_state("Failed"), None);
    assert_eq!(next_goal_for_state("Unknown"), Some("plan"));
}

/// Resume from Planned: skip plan, run acceptance-tests -> red -> green.
#[test]
fn full_workflow_resume_from_planned() {
    let plan_dir = std::env::temp_dir().join("tddy-full-resume-planned");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("TODO.md"), "# TODO\n- [ ] Task 1").expect("write TODO");
    write_changeset_with_state(&plan_dir, "Planned", "sess-resume-planned");

    let backend = MockBackend::new();
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let mut workflow = Workflow::new(backend);
    let at_options = AcceptanceTestsOptions::default();
    let _ = workflow
        .acceptance_tests(&plan_dir, None, &at_options)
        .expect("acceptance_tests should succeed");

    let red_options = RedOptions::default();
    let _ = workflow
        .red(&plan_dir, None, &red_options)
        .expect("red should succeed");

    let green_options = GreenOptions::default();
    let green_output = workflow
        .green(&plan_dir, None, &green_options)
        .expect("green should succeed");

    assert!(green_output.summary.contains("passing"));
    assert_eq!(
        workflow.backend().invocations().len(),
        3,
        "plan should be skipped"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Resume from AcceptanceTestsReady: skip plan and acceptance-tests, run red -> green.
#[test]
fn full_workflow_resume_from_acceptance_tests_ready() {
    let plan_dir = std::env::temp_dir().join("tddy-full-resume-at");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        plan_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- login_stores_session_token",
    )
    .expect("write acceptance-tests.md");
    write_changeset_with_state(&plan_dir, "AcceptanceTestsReady", "sess-resume-at");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let mut workflow = Workflow::new(backend);
    let red_options = RedOptions::default();
    let _ = workflow
        .red(&plan_dir, None, &red_options)
        .expect("red should succeed");

    let green_options = GreenOptions::default();
    let green_output = workflow
        .green(&plan_dir, None, &green_options)
        .expect("green should succeed");

    assert!(green_output.summary.contains("passing"));
    assert_eq!(
        workflow.backend().invocations().len(),
        2,
        "plan and at should be skipped"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// When state is GreenComplete, next_goal_for_state returns None (workflow complete).
#[test]
fn full_workflow_resume_from_green_complete_returns_none() {
    assert_eq!(next_goal_for_state("GreenComplete"), None);
}
