//! Integration tests for the full workflow (plan -> acceptance-tests -> red -> green).
//!
//! These acceptance tests define the expected behavior when running the full workflow
//! without a specific --goal. They verify chaining, resume logic, and next_goal_for_state.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use common::{
    ctx_acceptance_tests, ctx_demo, ctx_evaluate, ctx_green, ctx_plan, ctx_red, ctx_refactor,
    ctx_validate, plan_dir_for_input, run_goal_until_done, run_plan, write_changeset_with_state,
};
use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::output::parse_green_response;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{next_goal_for_state, MockBackend, SharedBackend, StubBackend, WorkflowEngine};

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

## TODO

- [ ] Create auth module
- [ ] Implement login endpoint
---PRD_END---

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

const EVALUATE_OUTPUT_CHAIN: &str = r#"Evaluation complete.

<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Evaluated. All criteria met.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[],"test_impact":{"tests_affected":0,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"OK"}
</structured-response>
"#;

/// Full workflow chains plan -> acceptance-tests -> red -> green on a single WorkflowEngine.
/// Uses run_workflow_from so the graph chains all steps in one run.
/// Now expects ElicitationNeeded after plan, then resume completes the rest.
#[tokio::test]
async fn full_workflow_chains_all_steps() {
    let output_dir = std::env::temp_dir().join("tddy-full-workflow-chain");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let plan_dir = plan_dir_for_input(&output_dir, "Build auth");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT_CHAIN);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-full-chain-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_plan("Build auth", output_dir.clone(), None, None);
    let result = engine.run_workflow_from("plan", ctx).await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "plan should trigger ElicitationNeeded, got {:?}",
        result.status
    );

    let mut result = engine.run_session(&result.session_id).await.unwrap();
    loop {
        match &result.status {
            ExecutionStatus::Completed => break,
            ExecutionStatus::Paused { .. } => {
                result = engine.run_session(&result.session_id).await.unwrap();
            }
            other => panic!("unexpected status: {:?}", other),
        }
    }

    let inv_count = backend.invocations().len();
    assert_eq!(
        inv_count, 8,
        "plan+at+red+green+evaluate+validate+refactor+update-docs (no demo), got {}",
        inv_count
    );
    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert_eq!(changeset.state.current, "DocsUpdated");

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Full workflow with StubBackend reaches GreenComplete (tddy-demo flow).
/// StubBackend always asks clarification (plan) and permission (acceptance-tests).
#[tokio::test]
async fn full_workflow_with_stub_backend_reaches_green_complete() {
    let output_dir = std::env::temp_dir().join("tddy-full-workflow-stub");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(StubBackend::new());
    let storage_dir = std::env::temp_dir().join("tddy-full-stub-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let first = run_plan(&engine, "Add a feature", &output_dir, None).await;
    assert!(
        first.is_err(),
        "plan should ask clarification first (StubBackend)"
    );

    let (plan_dir, _) = run_plan(
        &engine,
        "Add a feature",
        &output_dir,
        Some("Email/password"),
    )
    .await
    .expect("plan should succeed with clarification answer");

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let result = engine.run_goal("acceptance-tests", ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::WaitingForInput { .. }),
        "acceptance-tests should ask permission first"
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), Some("Yes"), false);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    let ctx = ctx_red(plan_dir.clone(), None);
    let result = run_goal_until_done(&engine, "red", ctx).await.unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    let ctx = ctx_green(plan_dir.clone(), None, false);
    let result = run_goal_until_done(&engine, "green", ctx).await.unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert!(
        matches!(
            changeset.state.current.as_str(),
            "GreenComplete" | "Evaluated" | "ValidateComplete" | "RefactorComplete" | "DocsUpdated"
        ),
        "StubBackend chain should reach GreenComplete or beyond, got {}",
        changeset.state.current
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

/// Resume from Planned: skip plan, run acceptance-tests -> red -> green -> evaluate -> validate -> refactor.
#[tokio::test]
async fn full_workflow_resume_from_planned() {
    let plan_dir = std::env::temp_dir().join("tddy-full-resume-planned");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("PRD.md"),
        "# PRD\n## Testing Plan\n\n## TODO\n\n- [ ] Task 1",
    )
    .expect("write PRD");
    write_changeset_with_state(&plan_dir, "Planned", "sess-resume-planned");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT_CHAIN);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-resume-planned-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        backend.invocations().len(),
        7,
        "plan skipped; at, red, green, evaluate, validate, refactor, update-docs"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Resume from AcceptanceTestsReady: skip plan and acceptance-tests, run red -> green -> evaluate -> validate -> refactor -> update-docs.
#[tokio::test]
async fn full_workflow_resume_from_acceptance_tests_ready() {
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

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT_CHAIN);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-resume-at-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_red(plan_dir.clone(), None);
    let result = run_goal_until_done(&engine, "red", ctx).await.unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        backend.invocations().len(),
        6,
        "plan and at skipped; red, green, evaluate, validate, refactor, update-docs"
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

/// AC1, AC7: Full workflow chains plan → acceptance-tests → red → green → demo → evaluate → validate → refactor
#[tokio::test]
async fn full_workflow_includes_demo_and_evaluate() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-demo-evaluate");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-full-demo-eval-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let (plan_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nCLI runs.",
    )
    .expect("write demo-plan.md");

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, true);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        read_changeset(&plan_dir).unwrap().state.current,
        "DocsUpdated"
    );
    assert_eq!(backend.invocations().len(), 9);

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// AC2, AC3: Full workflow where demo is skipped proceeds green → evaluate → validate → refactor → update-docs.
#[tokio::test]
async fn full_workflow_skip_demo_goes_to_evaluate() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-skip-demo");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-skip-demo-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let (plan_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        read_changeset(&plan_dir).unwrap().state.current,
        "DocsUpdated"
    );
    assert_eq!(backend.invocations().len(), 8);

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// AC7, AC8, AC9: Unit test for next_goal_for_state mapping.
#[test]
fn next_goal_for_state_includes_demo_and_evaluate() {
    assert_eq!(next_goal_for_state("GreenComplete"), Some("demo"));
    assert_eq!(next_goal_for_state("DemoComplete"), Some("evaluate"));
    assert_eq!(next_goal_for_state("Evaluated"), Some("validate"));
}

/// AC11: GreenOptions has no run_demo field.
#[test]
fn green_options_no_run_demo_field() {
    use tddy_core::GreenOptions;
    let _explicit = GreenOptions {
        model: None,
        agent_output: false,
        agent_output_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        allowed_tools_extras: None,
        debug: false,
    };
    let options = GreenOptions::default();
    assert!(options.model.is_none());
}

/// AC (R6): next_goal_for_state("Evaluated") returns Some("validate").
#[test]
fn next_goal_evaluated_returns_validate() {
    assert_eq!(next_goal_for_state("Evaluated"), Some("validate"));
}

/// AC (R6): next_goal_for_state("ValidateComplete") returns Some("refactor").
#[test]
fn next_goal_validate_complete_returns_refactor() {
    assert_eq!(next_goal_for_state("ValidateComplete"), Some("refactor"));
}

/// AC (R6): next_goal_for_state("RefactorComplete") returns Some("update-docs").
#[test]
fn next_goal_refactor_complete_returns_update_docs() {
    assert_eq!(next_goal_for_state("RefactorComplete"), Some("update-docs"));
}

/// next_goal_for_state("DocsUpdated") returns None (terminal).
#[test]
fn next_goal_docs_updated_returns_none() {
    assert_eq!(next_goal_for_state("DocsUpdated"), None);
}

/// AC10: plain full workflow uses a single WorkflowEngine instance.
/// With hook-triggered elicitation: plan returns ElicitationNeeded, then resume completes.
#[tokio::test]
async fn plain_full_workflow_uses_single_workflow_instance() {
    let output_dir = std::env::temp_dir().join("tddy-single-instance-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-single-instance-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let plan_dir = plan_dir_for_input(&output_dir, "Build auth");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    let mut ctx = ctx_plan("Build auth", output_dir.clone(), None, None);
    ctx.insert("run_demo".to_string(), serde_json::json!(true));
    let result = engine.run_workflow_from("plan", ctx).await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "plan should return ElicitationNeeded, got {:?}",
        result.status
    );

    let mut result = engine.run_session(&result.session_id).await.unwrap();
    loop {
        match &result.status {
            ExecutionStatus::Completed => break,
            ExecutionStatus::Paused { .. } => {
                result = engine.run_session(&result.session_id).await.unwrap();
            }
            other => panic!("unexpected status: {:?}", other),
        }
    }

    let invocations = backend.invocations();
    assert_eq!(invocations.len(), 9);

    let goals: Vec<_> = invocations.iter().map(|inv| inv.goal).collect();
    assert_eq!(goals[0], tddy_core::Goal::Plan);
    assert_eq!(goals[1], tddy_core::Goal::AcceptanceTests);
    assert_eq!(goals[2], tddy_core::Goal::Red);
    assert_eq!(goals[3], tddy_core::Goal::Green);
    assert_eq!(goals[4], tddy_core::Goal::Demo);
    assert_eq!(goals[5], tddy_core::Goal::Evaluate);
    assert_eq!(goals[6], tddy_core::Goal::Validate);
    assert_eq!(goals[7], tddy_core::Goal::Refactor);
    assert_eq!(goals[8], tddy_core::Goal::UpdateDocs);

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

const UPDATE_DOCS_OUTPUT: &str = r#"Documentation updated.

<structured-response content-type="application-json">
{"goal":"update-docs","summary":"Updated 3 docs.","docs_updated":3}
</structured-response>
"#;

/// Phase 5 / PRD R7: Full workflow chains all 9 steps (plan through update-docs).
#[tokio::test]
async fn full_workflow_chains_all_eight_steps_with_validate_and_refactor() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-8-steps");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(DEMO_OUTPUT);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-full-9-steps-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let (plan_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nCLI runs.",
    )
    .expect("write demo-plan.md");

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, true);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        read_changeset(&plan_dir).unwrap().state.current,
        "DocsUpdated"
    );
    assert_eq!(backend.invocations().len(), 9);

    let goals: Vec<_> = backend.invocations().iter().map(|inv| inv.goal).collect();
    assert_eq!(goals[0], tddy_core::Goal::Plan);
    assert_eq!(goals[1], tddy_core::Goal::AcceptanceTests);
    assert_eq!(goals[2], tddy_core::Goal::Red);
    assert_eq!(goals[3], tddy_core::Goal::Green);
    assert_eq!(goals[4], tddy_core::Goal::Demo);
    assert_eq!(goals[5], tddy_core::Goal::Evaluate);
    assert_eq!(goals[6], tddy_core::Goal::Validate);
    assert_eq!(goals[7], tddy_core::Goal::Refactor);
    assert_eq!(goals[8], tddy_core::Goal::UpdateDocs);

    let _ = std::fs::remove_dir_all(&output_dir);
}

// ── Hook-Triggered Elicitation acceptance tests ──────────────────────────────

/// TddWorkflowHooks triggers PlanApproval after plan task when PRD.md exists.
#[tokio::test]
async fn tdd_hooks_elicitation_returns_plan_approval_when_prd_exists() {
    let plan_dir = std::env::temp_dir().join("tddy-hooks-elicit-prd");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# Test PRD\n\nContent here.").expect("write PRD");

    let hooks = TddWorkflowHooks::new();
    let ctx = tddy_core::workflow::context::Context::new();
    ctx.set_sync("plan_dir", plan_dir.clone());

    let result = tddy_core::workflow::task::TaskResult {
        task_id: "plan".to_string(),
        response: "done".to_string(),
        next_action: tddy_core::workflow::task::NextAction::Continue,
        status_message: None,
    };

    let event = hooks.elicitation_after_task("plan", &ctx, &result);
    assert!(
        event.is_some(),
        "elicitation_after_task should return Some when PRD.md exists"
    );
    if let Some(tddy_core::ElicitationEvent::PlanApproval { prd_content }) = event {
        assert!(
            prd_content.contains("Test PRD"),
            "prd_content should contain PRD file contents"
        );
    }

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// TddWorkflowHooks returns None for non-plan tasks.
#[tokio::test]
async fn tdd_hooks_elicitation_returns_none_for_non_plan_tasks() {
    let plan_dir = std::env::temp_dir().join("tddy-hooks-elicit-nonplan");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD").expect("write PRD");

    let hooks = TddWorkflowHooks::new();
    let ctx = tddy_core::workflow::context::Context::new();
    ctx.set_sync("plan_dir", plan_dir.clone());

    let result = tddy_core::workflow::task::TaskResult {
        task_id: "red".to_string(),
        response: "done".to_string(),
        next_action: tddy_core::workflow::task::NextAction::Continue,
        status_message: None,
    };

    assert!(
        hooks.elicitation_after_task("red", &ctx, &result).is_none(),
        "elicitation_after_task should return None for non-plan tasks"
    );
    assert!(hooks
        .elicitation_after_task("acceptance-tests", &ctx, &result)
        .is_none(),);
    assert!(hooks
        .elicitation_after_task("green", &ctx, &result)
        .is_none(),);

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// TddWorkflowHooks returns None when PRD.md doesn't exist.
#[tokio::test]
async fn tdd_hooks_elicitation_returns_none_when_no_prd() {
    let plan_dir = std::env::temp_dir().join("tddy-hooks-elicit-noprd");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let hooks = TddWorkflowHooks::new();
    let ctx = tddy_core::workflow::context::Context::new();
    ctx.set_sync("plan_dir", plan_dir.clone());

    let result = tddy_core::workflow::task::TaskResult {
        task_id: "plan".to_string(),
        response: "done".to_string(),
        next_action: tddy_core::workflow::task::NextAction::Continue,
        status_message: None,
    };

    assert!(
        hooks
            .elicitation_after_task("plan", &ctx, &result)
            .is_none(),
        "elicitation_after_task should return None when PRD.md doesn't exist"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Full workflow with TddWorkflowHooks: engine returns ElicitationNeeded after plan.
#[tokio::test]
async fn full_workflow_returns_elicitation_needed_after_plan() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-elicit");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-full-elicit-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_plan("Build auth", output_dir.clone(), None, None);
    let result = engine.run_workflow_from("plan", ctx).await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "full workflow should return ElicitationNeeded after plan, got {:?}",
        result.status
    );

    if let ExecutionStatus::ElicitationNeeded { ref event } = result.status {
        match event {
            tddy_core::ElicitationEvent::PlanApproval { ref prd_content } => {
                assert!(!prd_content.is_empty(), "prd_content should not be empty");
            }
        }
    }

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

/// Full workflow: after ElicitationNeeded, caller can resume and workflow continues.
#[tokio::test]
async fn full_workflow_resumes_after_elicitation_approval() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-elicit-resume");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT_CHAIN);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-full-elicit-resume-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_plan("Build auth", output_dir.clone(), None, None);
    let result = engine.run_workflow_from("plan", ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "first call should return ElicitationNeeded"
    );

    let mut result = engine.run_session(&result.session_id).await.unwrap();
    loop {
        match &result.status {
            ExecutionStatus::Completed => break,
            ExecutionStatus::ElicitationNeeded { .. } | ExecutionStatus::WaitingForInput { .. } => {
                panic!("unexpected status after resume: {:?}", result.status);
            }
            ExecutionStatus::Error(e) => panic!("unexpected error: {}", e),
            ExecutionStatus::Paused { .. } => {
                result = engine.run_session(&result.session_id).await.unwrap();
            }
        }
    }

    assert_eq!(
        backend.invocations().len(),
        8,
        "plan+at+red+green+evaluate+validate+refactor+update-docs"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

/// Phase 5: Full workflow with skipped demo still includes validate, refactor, and update-docs.
#[tokio::test]
async fn full_workflow_skip_demo_includes_validate_and_refactor() {
    let output_dir = std::env::temp_dir().join("tddy-full-wf-skip-demo-8");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_SUBAGENTS_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT_COMPLETE);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-skip-demo-8-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir.clone(),
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let (plan_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan");

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let result = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();
    assert!(!matches!(result.status, ExecutionStatus::Error(_)));

    assert_eq!(
        read_changeset(&plan_dir).unwrap().state.current,
        "DocsUpdated"
    );
    assert_eq!(backend.invocations().len(), 8);

    let _ = std::fs::remove_dir_all(&output_dir);
}
