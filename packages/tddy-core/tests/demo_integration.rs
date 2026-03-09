//! Integration tests for the standalone demo goal (TDD Workflow Restructure PRD).
//!
//! These tests verify the demo() method, DemoOptions struct,
//! Goal::Demo variant, parse_demo_response, and state transitions for the demo step.
//! All tests are expected to FAIL in the Red phase.

use tddy_core::{
    DemoOptions, GreenOptions, MockBackend, PlanOptions, RedOptions, Workflow, WorkflowState,
};

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
---PRD_END---

---TODO_START---
- [ ] Create auth module
---TODO_END---

That concludes the plan."#;

const GREEN_OUTPUT_ALL_PASS: &str = r#"Implemented production code. All tests passing.

<structured-response content-type="application-json">
{"goal":"green","summary":"All 3 tests passing.","tests":[{"name":"test_auth","file":"src/auth.rs","line":42,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"Created acceptance tests.

<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 1 acceptance test.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"}]}
</structured-response>
"#;

const RED_OUTPUT: &str = r#"Created skeleton code.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}
</structured-response>
"#;

const DEMO_OUTPUT: &str = r#"Demo executed successfully.

<structured-response content-type="application-json">
{"goal":"demo","summary":"Demo ran successfully. CLI produced expected output.","demo_type":"cli","steps_completed":2,"verification":"CLI runs without error"}
</structured-response>
"#;

const EVALUATE_OUTPUT: &str = r#"Evaluation complete.

<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Evaluated 3 changed files. All criteria met.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/workflow/mod.rs","lines_changed":50,"changeset_item":null}],"test_impact":{"tests_affected":5,"new_tests_needed":0},"changed_files":[{"path":"src/workflow/mod.rs","change_type":"modified","lines_added":30,"lines_removed":10}],"affected_tests":[{"path":"tests/full_workflow_integration.rs","status":"updated","description":"Updated for new demo/evaluate flow"}],"validity_assessment":"All acceptance criteria from the PRD are addressed."}
</structured-response>
"#;

fn setup_plan_dir_with_green_complete(label: &str) -> (std::path::PathBuf, Workflow<MockBackend>) {
    let output_dir = std::env::temp_dir().join(format!("tddy-demo-{}", label));
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = MockBackend::new();
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let mut workflow = Workflow::new(backend);
    let plan_dir = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan");

    let _ = workflow.acceptance_tests(
        &plan_dir,
        None,
        &tddy_core::AcceptanceTestsOptions::default(),
    );
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());
    let _ = workflow.green(&plan_dir, None, &GreenOptions::default());

    (plan_dir, workflow)
}

// ── Tests that verify new behavior not yet implemented (all should FAIL) ──────

/// Demo should parse the structured response and return DemoOutput.
/// Currently parse_demo_response has todo!() and the output is not parsed.
/// This test verifies the demo() method completes successfully end-to-end.
#[test]
fn demo_completes_and_returns_demo_output() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M004\",\"scope\":\"demo_integration::demo_completes_and_returns_demo_output\",\"data\":{{}}}}}}");
    let (plan_dir, mut workflow) = setup_plan_dir_with_green_complete("demo-output");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    workflow.backend().push_ok(DEMO_OUTPUT);

    let result = workflow.demo(&plan_dir, None, &DemoOptions::default());
    assert!(
        result.is_ok(),
        "demo should complete successfully and return DemoOutput, got {:?}",
        result
    );

    let output = result.unwrap();
    assert_eq!(
        output.summary,
        "Demo ran successfully. CLI produced expected output."
    );
    assert_eq!(output.demo_type, "cli");
    assert_eq!(output.steps_completed, 2);
}

/// After demo completes, state should be DemoComplete.
#[test]
fn demo_sets_state_to_demo_complete() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M005\",\"scope\":\"demo_integration::demo_sets_state_to_demo_complete\",\"data\":{{}}}}}}");
    let (plan_dir, mut workflow) = setup_plan_dir_with_green_complete("demo-state");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    workflow.backend().push_ok(DEMO_OUTPUT);

    let _ = workflow.demo(&plan_dir, None, &DemoOptions::default());

    assert!(
        matches!(workflow.state(), WorkflowState::DemoComplete { .. }),
        "after demo, state should be DemoComplete, got {:?}",
        workflow.state()
    );
}

/// next_goal_for_state: GreenComplete should map to "demo" (not None).
#[test]
fn next_goal_green_complete_maps_to_demo() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M006\",\"scope\":\"demo_integration::next_goal_green_complete_maps_to_demo\",\"data\":{{}}}}}}");
    let result = tddy_core::next_goal_for_state("GreenComplete");
    assert_eq!(
        result,
        Some("demo"),
        "GreenComplete should map to Some(\"demo\"), got {:?}",
        result
    );
}

/// next_goal_for_state: DemoComplete should map to "evaluate".
#[test]
fn next_goal_demo_complete_maps_to_evaluate() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M007\",\"scope\":\"demo_integration::next_goal_demo_complete_maps_to_evaluate\",\"data\":{{}}}}}}");
    let result = tddy_core::next_goal_for_state("DemoComplete");
    assert_eq!(
        result,
        Some("evaluate"),
        "DemoComplete should map to Some(\"evaluate\"), got {:?}",
        result
    );
}

/// default_models() should include a "demo" key.
#[test]
fn default_models_includes_demo() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M009\",\"scope\":\"demo_integration::default_models_includes_demo\",\"data\":{{}}}}}}");
    let changeset = tddy_core::Changeset::default();
    assert!(
        changeset.models.contains_key("demo"),
        "default_models should include 'demo' key, got keys: {:?}",
        changeset.models.keys().collect::<Vec<_>>()
    );
}

/// evaluate() should accept GreenComplete state (needed for full workflow).
/// Currently evaluate only starts from Init — this needs to be relaxed.
#[test]
fn evaluate_accepts_green_complete_state() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M010\",\"scope\":\"demo_integration::evaluate_accepts_green_complete_state\",\"data\":{{}}}}}}");
    let (plan_dir, mut workflow) = setup_plan_dir_with_green_complete("eval-from-green");

    workflow.backend().push_ok(EVALUATE_OUTPUT);

    let result = workflow.evaluate(
        &std::path::Path::new("."),
        Some(&plan_dir),
        None,
        &tddy_core::EvaluateOptions::default(),
    );
    assert!(
        result.is_ok(),
        "evaluate should accept GreenComplete state for full workflow, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

/// evaluate() should accept DemoComplete state (needed for full workflow after demo).
#[test]
fn evaluate_accepts_demo_complete_state() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M011\",\"scope\":\"demo_integration::evaluate_accepts_demo_complete_state\",\"data\":{{}}}}}}");
    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);
    let mut workflow = Workflow::new(backend);
    workflow.restore_state(WorkflowState::DemoComplete {
        output: tddy_core::DemoOutput {
            summary: "test".to_string(),
            demo_type: "cli".to_string(),
            steps_completed: 1,
            verification: "ok".to_string(),
        },
    });

    let plan_dir = std::env::temp_dir().join("tddy-demo-eval-accepts-dc");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create dir");

    let result = workflow.evaluate(
        &std::path::Path::new("."),
        Some(&plan_dir),
        None,
        &tddy_core::EvaluateOptions::default(),
    );
    assert!(
        result.is_ok(),
        "evaluate should accept DemoComplete state, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Demo backend invocation should use Goal::Demo.
/// Verify the invocation's goal field when demo completes.
#[test]
fn demo_backend_invocation_uses_goal_demo() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M013\",\"scope\":\"demo_integration::demo_backend_invocation_uses_goal_demo\",\"data\":{{}}}}}}");
    let (plan_dir, mut workflow) = setup_plan_dir_with_green_complete("goal-demo");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- test\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    workflow.backend().push_ok(DEMO_OUTPUT);

    let result = workflow.demo(&plan_dir, None, &DemoOptions::default());
    assert!(
        result.is_ok(),
        "demo should succeed (requires parse_demo_response to be implemented), got {:?}",
        result
    );

    let invocations = workflow.backend().invocations();
    let demo_inv = invocations.last().expect("should have demo invocation");
    assert_eq!(
        demo_inv.goal,
        tddy_core::Goal::Demo,
        "demo invocation should use Goal::Demo"
    );
}

/// Changeset should include "demo" in default_models for model resolution.
#[test]
fn changeset_default_models_has_demo_key() {
    eprintln!("{{\"tddy\":{{\"marker_id\":\"M014\",\"scope\":\"demo_integration::changeset_default_models_has_demo_key\",\"data\":{{}}}}}}");
    let cs = tddy_core::Changeset::default();
    let demo_model = tddy_core::resolve_model(Some(&cs), "demo", None);
    assert!(
        demo_model.is_some(),
        "resolve_model for 'demo' should return a model from default_models, got None"
    );
}
