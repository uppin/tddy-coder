//! Integration tests for the full workflow (plan -> acceptance-tests -> red -> green).
//!
//! These acceptance tests define the expected behavior when running the full workflow
//! without a specific --goal. They verify chaining, resume logic, and next_goal_for_state.

use tddy_core::{
    next_goal_for_state, AcceptanceTestsOptions, DemoOptions, EvaluateOptions, MockBackend,
    PlanOptions, RedOptions, RefactorOptions, ValidateOptions, Workflow,
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
    assert_eq!(next_goal_for_state("GreenComplete"), Some("demo"));
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

/// When state is GreenComplete, next_goal_for_state returns "demo" (then evaluate).
#[test]
fn full_workflow_resume_from_green_complete_returns_demo() {
    assert_eq!(next_goal_for_state("GreenComplete"), Some("demo"));
}

// ── Acceptance tests for TDD Workflow Restructure PRD ─────────────────────────

const EVALUATE_OUTPUT: &str = r#"Evaluation complete.

<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Evaluated 3 changed files. All criteria met.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/workflow/mod.rs","lines_changed":50,"changeset_item":null}],"test_impact":{"tests_affected":5,"new_tests_needed":0},"changed_files":[{"path":"src/workflow/mod.rs","change_type":"modified","lines_added":30,"lines_removed":10}],"affected_tests":[{"path":"tests/full_workflow_integration.rs","status":"updated","description":"Updated for new demo/evaluate flow"}],"validity_assessment":"All acceptance criteria from the PRD are addressed."}
</structured-response>
"#;

const DEMO_OUTPUT: &str = r#"Demo executed successfully.

<structured-response content-type="application-json">
{"goal":"demo","summary":"Demo ran successfully. CLI produced expected output.","demo_type":"cli","steps_completed":2,"verification":"CLI runs without error"}
</structured-response>
"#;

/// AC1, AC7: Full workflow chains plan → acceptance-tests → red → green → demo → evaluate
/// with MockBackend. All 6+ goals invoked in order; final state is Evaluated.
///
/// This test will fail until:
/// - DemoComplete/DemoRunning states are in WorkflowState
/// - workflow.demo() method is implemented
/// - next_goal_for_state maps GreenComplete → "demo" and DemoComplete → "evaluate"
/// - evaluate can run from GreenComplete/DemoComplete state (not just Init)
#[test]
fn full_workflow_includes_demo_and_evaluate() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-demo-evaluate");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);

    let plan_options = PlanOptions::default();
    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &plan_options)
        .expect("plan should succeed");

    // Write demo-plan.md so demo goal can find it
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nCLI runs.",
    )
    .expect("write demo-plan.md");

    let at_options = AcceptanceTestsOptions::default();
    let _ = workflow
        .acceptance_tests(&plan_dir, None, &at_options)
        .expect("acceptance_tests should succeed");

    let red_options = RedOptions::default();
    let _ = workflow
        .red(&plan_dir, None, &red_options)
        .expect("red should succeed");

    let green_options = GreenOptions::default();
    let _ = workflow
        .green(&plan_dir, None, &green_options)
        .expect("green should succeed");

    // After green, state should be GreenComplete
    assert!(
        matches!(workflow.state(), WorkflowState::GreenComplete { .. }),
        "after green, state should be GreenComplete, got {:?}",
        workflow.state()
    );

    // Demo step — requires workflow.demo() method to exist
    let demo_result = workflow.demo(&plan_dir, None, &DemoOptions::default());
    assert!(
        demo_result.is_ok(),
        "demo should succeed, got {:?}",
        demo_result
    );

    // After demo, state should be DemoComplete
    assert!(
        matches!(workflow.state(), WorkflowState::DemoComplete { .. }),
        "after demo, state should be DemoComplete, got {:?}",
        workflow.state()
    );

    // Evaluate step — requires evaluate to accept DemoComplete state
    let eval_options = EvaluateOptions::default();
    let eval_result = workflow.evaluate(
        &std::path::Path::new("."),
        Some(&plan_dir),
        None,
        &eval_options,
    );
    assert!(
        eval_result.is_ok(),
        "evaluate should succeed, got {:?}",
        eval_result
    );

    // Final state should be Evaluated
    assert!(
        matches!(workflow.state(), WorkflowState::Evaluated { .. }),
        "final state should be Evaluated, got {:?}",
        workflow.state()
    );

    // All 6 goals should have been invoked
    assert_eq!(
        workflow.backend().invocations().len(),
        6,
        "all 6 goals should be invoked: plan, acceptance-tests, red, green, demo, evaluate"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// AC2, AC3: Full workflow where demo is skipped proceeds green → evaluate.
/// When user skips demo, workflow goes directly from GreenComplete to evaluate.
#[test]
fn full_workflow_skip_demo_goes_to_evaluate() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-skip-demo");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);

    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan should succeed");

    let _ = workflow
        .acceptance_tests(&plan_dir, None, &AcceptanceTestsOptions::default())
        .expect("acceptance_tests should succeed");

    let _ = workflow
        .red(&plan_dir, None, &RedOptions::default())
        .expect("red should succeed");

    let _ = workflow
        .green(&plan_dir, None, &GreenOptions::default())
        .expect("green should succeed");

    // Skip demo: go directly from GreenComplete to evaluate (no DemoSkipped state)
    let eval_result = workflow.evaluate(
        &std::path::Path::new("."),
        Some(&plan_dir),
        None,
        &EvaluateOptions::default(),
    );
    assert!(
        eval_result.is_ok(),
        "evaluate should succeed when demo is skipped, got {:?}",
        eval_result
    );

    assert!(
        matches!(workflow.state(), WorkflowState::Evaluated { .. }),
        "final state should be Evaluated, got {:?}",
        workflow.state()
    );

    assert_eq!(
        workflow.backend().invocations().len(),
        5,
        "5 goals when demo skipped: plan, acceptance-tests, red, green, evaluate"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// AC7, AC8, AC9: Unit test for next_goal_for_state mapping.
/// GreenComplete → "demo"; DemoComplete → "evaluate"; Evaluated → "validate".
#[test]
fn next_goal_for_state_includes_demo_and_evaluate() {
    assert_eq!(
        next_goal_for_state("GreenComplete"),
        Some("demo"),
        "GreenComplete should map to demo"
    );
    assert_eq!(
        next_goal_for_state("DemoComplete"),
        Some("evaluate"),
        "DemoComplete should map to evaluate"
    );
    assert_eq!(
        next_goal_for_state("Evaluated"),
        Some("validate"),
        "Evaluated should map to validate"
    );
}

/// AC11: GreenOptions has no run_demo field.
/// GreenOptions::default() should compile without run_demo; green focuses purely on implementation.
///
/// This test will fail until:
/// - The run_demo field is removed from GreenOptions
#[test]
fn green_options_no_run_demo_field() {
    // Construct GreenOptions with all expected fields via struct literal.
    // If run_demo still exists on the struct, this is non-exhaustive and fails to compile.
    let _explicit = GreenOptions {
        model: None,
        agent_output: false,
        agent_output_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        allowed_tools_extras: None,
        debug: false,
        // run_demo is intentionally NOT listed here — if the field still exists,
        // this struct literal is non-exhaustive and will fail to compile.
    };

    let options = GreenOptions::default();
    // Verify default works and is usable
    assert!(
        options.model.is_none(),
        "GreenOptions::default() model should be None"
    );
}

/// AC (R6): next_goal_for_state("Evaluated") returns Some("validate").
///
/// This test will fail until:
/// - next_goal_for_state is updated to return Some("validate") for "Evaluated"
///   (currently returns None)
#[test]
fn next_goal_evaluated_returns_validate() {
    assert_eq!(
        next_goal_for_state("Evaluated"),
        Some("validate"),
        "Evaluated should map to validate"
    );
}

/// AC (R6): next_goal_for_state("ValidateComplete") returns Some("refactor").
///
/// This test will fail until:
/// - "ValidateComplete" state is handled in next_goal_for_state
/// - It returns Some("refactor")
#[test]
fn next_goal_validate_complete_returns_refactor() {
    assert_eq!(
        next_goal_for_state("ValidateComplete"),
        Some("refactor"),
        "ValidateComplete should map to refactor"
    );
}

/// AC (R6): next_goal_for_state("RefactorComplete") returns None (terminal).
///
/// This test will fail until:
/// - "RefactorComplete" state is handled in next_goal_for_state
/// - It returns None
#[test]
fn next_goal_refactor_complete_returns_none() {
    assert_eq!(
        next_goal_for_state("RefactorComplete"),
        None,
        "RefactorComplete should be terminal (None)"
    );
}

/// AC10: plain full workflow uses a single Workflow instance.
/// Backend invocation count matches expected (no dropped invocations from a second instance).
///
/// This test verifies that all goals share a single Workflow instance by checking
/// the total invocation count matches exactly what was pushed onto the MockBackend.
///
/// This test will fail until:
/// - run_full_workflow_plain uses a single Workflow instance
/// - demo and evaluate are included in the full workflow
#[test]
fn plain_full_workflow_uses_single_workflow_instance() {
    let output_dir = std::env::temp_dir().join("tddy-single-instance-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);

    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    let _ = workflow.acceptance_tests(&plan_dir, None, &AcceptanceTestsOptions::default());
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let _ = workflow.green(&plan_dir, None, &GreenOptions::default());
    let _ = workflow.demo(&plan_dir, None, &DemoOptions::default());
    let _ = workflow.evaluate(
        &std::path::Path::new("."),
        Some(&plan_dir),
        None,
        &EvaluateOptions::default(),
    );

    // Single instance: all invocations should be tracked on the same backend
    let invocations = workflow.backend().invocations();
    assert_eq!(
        invocations.len(),
        6,
        "single Workflow instance should track all 6 invocations (plan, at, red, green, demo, evaluate), got {}",
        invocations.len()
    );

    // Verify goal order
    let goals: Vec<_> = invocations.iter().map(|inv| inv.goal.clone()).collect();
    assert_eq!(goals[0], tddy_core::Goal::Plan, "first goal should be Plan");
    assert_eq!(
        goals[1],
        tddy_core::Goal::AcceptanceTests,
        "second goal should be AcceptanceTests"
    );
    assert_eq!(goals[2], tddy_core::Goal::Red, "third goal should be Red");
    assert_eq!(
        goals[3],
        tddy_core::Goal::Green,
        "fourth goal should be Green"
    );
    assert_eq!(goals[4], tddy_core::Goal::Demo, "fifth goal should be Demo");
    assert_eq!(
        goals[5],
        tddy_core::Goal::Evaluate,
        "sixth goal should be Evaluate"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

// ── Phase 5: Full workflow chains all 8 steps ───────────────────────────────

const VALIDATE_SUBAGENTS_OUTPUT: &str = r#"All 3 subagents completed. Reports and refactoring plan written.

<structured-response content-type="application-json">
{"goal":"validate","summary":"All 3 subagents completed. Reports and refactoring plan written.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}
</structured-response>
"#;

const REFACTOR_OUTPUT_COMPLETE: &str = r#"All refactoring tasks completed. Tests passing.

<structured-response content-type="application-json">
{"goal":"refactor","summary":"Completed 5 refactoring tasks. All tests passing.","tasks_completed":5,"tests_passing":true}
</structured-response>
"#;

/// Phase 5 / PRD R7: Full workflow chains all 8 steps:
/// plan → acceptance-tests → red → green → demo → evaluate → validate → refactor.
/// Final state should be RefactorComplete. 8 backend invocations total.
#[test]
fn full_workflow_chains_all_eight_steps_with_validate_and_refactor() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-8-steps");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);

    let mut workflow = Workflow::new(backend);

    // 1. Plan
    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan should succeed");

    // Write demo-plan.md so demo goal can find it
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nCLI runs.",
    )
    .expect("write demo-plan.md");

    // 2. Acceptance tests
    let _ = workflow
        .acceptance_tests(&plan_dir, None, &AcceptanceTestsOptions::default())
        .expect("acceptance_tests should succeed");

    // 3. Red
    let _ = workflow
        .red(&plan_dir, None, &RedOptions::default())
        .expect("red should succeed");

    // 4. Green
    let _ = workflow
        .green(&plan_dir, None, &GreenOptions::default())
        .expect("green should succeed");

    // 5. Demo
    let _ = workflow
        .demo(&plan_dir, None, &DemoOptions::default())
        .expect("demo should succeed");

    // 6. Evaluate (writes evaluation-report.md to plan_dir)
    let _ = workflow
        .evaluate(
            &std::path::Path::new("."),
            Some(&plan_dir),
            None,
            &EvaluateOptions::default(),
        )
        .expect("evaluate should succeed");

    assert!(
        matches!(workflow.state(), WorkflowState::Evaluated { .. }),
        "after evaluate, state should be Evaluated, got {:?}",
        workflow.state()
    );

    // 7. Validate (subagent-based, requires evaluation-report.md)
    let validate_result = workflow.validate(&plan_dir, None, &ValidateOptions::default());
    assert!(
        validate_result.is_ok(),
        "validate (subagent) should succeed, got: {:?}",
        validate_result
    );

    assert!(
        matches!(workflow.state(), WorkflowState::ValidateComplete { .. }),
        "after validate, state should be ValidateComplete, got {:?}",
        workflow.state()
    );

    // Write refactoring-plan.md (MockBackend doesn't write files; in production
    // the validate agent writes this via Write tool)
    std::fs::write(
        plan_dir.join("refactoring-plan.md"),
        "# Refactoring Plan\n## Tasks\n1. Extract shared workflow helper\n2. Add error context\n3. Remove magic strings\n4. Add doc comments\n5. Consolidate duplicated test setup",
    )
    .expect("write refactoring-plan.md");

    // 8. Refactor (requires refactoring-plan.md)
    let refactor_result = workflow.refactor(&plan_dir, None, &RefactorOptions::default());
    assert!(
        refactor_result.is_ok(),
        "refactor should succeed, got: {:?}",
        refactor_result
    );

    assert!(
        matches!(workflow.state(), WorkflowState::RefactorComplete { .. }),
        "final state should be RefactorComplete, got {:?}",
        workflow.state()
    );

    // All 8 goals should have been invoked
    assert_eq!(
        workflow.backend().invocations().len(),
        8,
        "all 8 goals should be invoked: plan, at, red, green, demo, evaluate, validate, refactor"
    );

    // Verify goal order
    let goals: Vec<_> = workflow
        .backend()
        .invocations()
        .iter()
        .map(|inv| inv.goal)
        .collect();
    assert_eq!(goals[0], tddy_core::Goal::Plan);
    assert_eq!(goals[1], tddy_core::Goal::AcceptanceTests);
    assert_eq!(goals[2], tddy_core::Goal::Red);
    assert_eq!(goals[3], tddy_core::Goal::Green);
    assert_eq!(goals[4], tddy_core::Goal::Demo);
    assert_eq!(goals[5], tddy_core::Goal::Evaluate);
    assert_eq!(goals[6], tddy_core::Goal::Validate);
    assert_eq!(goals[7], tddy_core::Goal::Refactor);

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Phase 5: Full workflow with skipped demo still includes validate and refactor.
/// plan → at → red → green → (skip demo) → evaluate → validate → refactor.
#[test]
fn full_workflow_skip_demo_includes_validate_and_refactor() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-skip-demo-8");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    // No demo output — demo is skipped
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);

    let mut workflow = Workflow::new(backend);

    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan");

    let _ = workflow.acceptance_tests(&plan_dir, None, &AcceptanceTestsOptions::default());
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let _ = workflow.green(&plan_dir, None, &GreenOptions::default());

    let _ = workflow
        .evaluate(
            &std::path::Path::new("."),
            Some(&plan_dir),
            None,
            &EvaluateOptions::default(),
        )
        .expect("evaluate should succeed");

    let _ = workflow
        .validate(&plan_dir, None, &ValidateOptions::default())
        .expect("validate should succeed");

    std::fs::write(
        plan_dir.join("refactoring-plan.md"),
        "# Refactoring Plan\n## Tasks\n1. Fix issues",
    )
    .expect("write refactoring-plan.md");

    let refactor_result = workflow.refactor(&plan_dir, None, &RefactorOptions::default());
    assert!(
        refactor_result.is_ok(),
        "refactor should succeed after skip-demo workflow: {:?}",
        refactor_result
    );

    assert!(
        matches!(workflow.state(), WorkflowState::RefactorComplete { .. }),
        "final state should be RefactorComplete, got {:?}",
        workflow.state()
    );

    // 7 backend invocations (demo skipped)
    assert_eq!(
        workflow.backend().invocations().len(),
        7,
        "7 goals should be invoked when demo is skipped"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
