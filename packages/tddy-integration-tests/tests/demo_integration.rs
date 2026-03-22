//! Integration tests for the standalone demo goal (TDD Workflow Restructure PRD).
//!
//! These tests verify the demo() method, DemoOptions struct,
//! Goal::Demo variant, parse_demo_response, and state transitions for the demo step.
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

use tddy_core::workflow::ids::WorkflowState;
use tddy_core::GoalId;

use common::{
    ctx_acceptance_tests, ctx_demo, ctx_evaluate, ctx_green, ctx_red, run_goal_until_done,
    run_plan, temp_dir_with_git_repo,
};
use tddy_workflow_recipes::TddWorkflowHooks;

fn tdd_recipe_default_models_str() -> BTreeMap<String, String> {
    common::tdd_recipe()
        .default_models()
        .into_iter()
        .map(|(k, v)| (k.as_str().to_string(), v))
        .collect()
}

/// Plan output as JSON (tddy-tools submit format). MockBackend stores via store_submit_result.
const PLAN_OUTPUT: &str = r##"{"goal":"plan","prd":"# Feature PRD\n\n## Summary\nUser authentication system with login and logout.\n\n## Testing Plan\n\n### Test Level\nIntegration - changes how auth component interacts with session storage.\n\n### Acceptance Tests\n- [ ] **Integration**: Login stores session token (packages/auth/tests/session.it.rs)\n\n## TODO\n\n- [ ] Create auth module","branch_suggestion":"feature/auth","worktree_suggestion":"feature-auth"}"##;

const GREEN_OUTPUT_ALL_PASS: &str = r#"{"goal":"green","summary":"All 3 tests passing.","tests":[{"name":"test_auth","file":"src/auth.rs","line":42,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"{"goal":"acceptance-tests","summary":"Created 1 acceptance test.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"}]}"#;

const RED_OUTPUT: &str = r#"{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;

const DEMO_OUTPUT: &str = r#"{"goal":"demo","summary":"Demo ran successfully. CLI produced expected output.","demo_type":"cli","steps_completed":2,"verification":"CLI runs without error"}"#;

const EVALUATE_OUTPUT: &str = r#"{"goal":"evaluate-changes","summary":"Evaluated 3 changed files. All criteria met.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/workflow/mod.rs","lines_changed":50,"changeset_item":null}],"test_impact":{"tests_affected":5,"new_tests_needed":0},"changed_files":[{"path":"src/workflow/mod.rs","change_type":"modified","lines_added":30,"lines_removed":10}],"affected_tests":[{"path":"tests/full_workflow_integration.rs","status":"updated","description":"Updated for new demo/evaluate flow"}],"validity_assessment":"All acceptance criteria from the PRD are addressed."}"#;

const VALIDATE_OUTPUT: &str = r#"{"goal":"validate","summary":"All 3 subagents completed.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

const REFACTOR_OUTPUT: &str = r#"{"goal":"refactor","summary":"Completed. All tests passing.","tasks_completed":5,"tests_passing":true}"#;

const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

async fn setup_session_dir_with_green_complete(
    label: &str,
) -> (std::path::PathBuf, WorkflowEngine, Arc<MockBackend>) {
    let (output_dir, _) = temp_dir_with_git_repo(&format!("demo-{}", label), "Build auth");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join(format!("tddy-demo-engine-{}", label));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (session_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan");

    // Run each step with run_goal (single step) so we stop at GreenComplete.
    let ctx = ctx_acceptance_tests(session_dir.clone(), Some(output_dir), None, false);
    let r = engine
        .run_goal(&GoalId::new("acceptance-tests"), ctx)
        .await
        .unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "at: {:?}",
        r.status
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let r = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "red: {:?}",
        r.status
    );

    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    let ctx = ctx_green(session_dir.clone(), None, false);
    let result = engine.run_goal(&GoalId::new("green"), ctx).await.unwrap();
    match &result.status {
        ExecutionStatus::Paused { .. } | ExecutionStatus::ElicitationNeeded { .. } => {}
        _ => panic!("expected Paused after green, got {:?}", result.status),
    }

    (session_dir, engine, backend)
}

/// Demo should parse the structured response and return DemoOutput.
#[tokio::test]
async fn demo_completes_and_returns_demo_output() {
    let (session_dir, engine, backend) = setup_session_dir_with_green_complete("demo-output").await;

    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    backend.push_ok(DEMO_OUTPUT);

    let ctx = ctx_demo(session_dir.clone());
    let result = engine.run_goal(&GoalId::new("demo"), ctx).await.unwrap();
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "demo: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output =
        tddy_workflow_recipes::parse_demo_response(&output_str).expect("parse demo output");
    assert_eq!(
        output.summary,
        "Demo ran successfully. CLI produced expected output."
    );
    assert_eq!(output.demo_type, "cli");
    assert_eq!(output.steps_completed, 2);

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

/// After demo completes, state should be DemoComplete.
#[tokio::test]
async fn demo_sets_state_to_demo_complete() {
    let (session_dir, engine, backend) = setup_session_dir_with_green_complete("demo-state").await;

    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    backend.push_ok(DEMO_OUTPUT);

    let ctx = ctx_demo(session_dir.clone());
    let r = engine.run_goal(&GoalId::new("demo"), ctx).await.unwrap();
    assert!(
        matches!(r.status, ExecutionStatus::Paused { .. }),
        "demo: {:?}",
        r.status
    );

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_eq!(
        changeset.state.current,
        WorkflowState::new("DemoComplete"),
        "after demo, state should be DemoComplete, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

/// next_goal_for_state: GreenComplete should map to "demo" (not None).
#[test]
fn next_goal_green_complete_maps_to_demo() {
    let recipe = common::tdd_recipe();
    let result = recipe.next_goal_for_state(&WorkflowState::new("GreenComplete"));
    assert_eq!(
        result,
        Some(GoalId::new("demo")),
        "GreenComplete should map to Some(\"demo\"), got {:?}",
        result
    );
}

/// next_goal_for_state: DemoComplete should map to "evaluate".
#[test]
fn next_goal_demo_complete_maps_to_evaluate() {
    let recipe = common::tdd_recipe();
    let result = recipe.next_goal_for_state(&WorkflowState::new("DemoComplete"));
    assert_eq!(
        result,
        Some(GoalId::new("evaluate")),
        "DemoComplete should map to Some(\"evaluate\"), got {:?}",
        result
    );
}

/// Recipe default_models includes "demo"; resolve_model falls back when changeset has no entry.
#[test]
fn default_models_includes_demo() {
    let changeset = tddy_core::Changeset::default();
    let defaults = tdd_recipe_default_models_str();
    assert!(
        defaults.contains_key("demo"),
        "TDD recipe default_models should include 'demo', got keys: {:?}",
        defaults.keys().collect::<Vec<_>>()
    );
    assert!(
        tddy_core::resolve_model(Some(&changeset), "demo", None, Some(&defaults)).is_some(),
        "resolve_model should fall back to recipe defaults for demo"
    );
}

/// evaluate() should accept GreenComplete state (needed for full workflow).
#[tokio::test]
async fn evaluate_accepts_green_complete_state() {
    let (session_dir, engine, backend) =
        setup_session_dir_with_green_complete("eval-from-green").await;

    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let ctx = ctx_evaluate(
        session_dir.clone(),
        Some(std::path::Path::new(".").to_path_buf()),
    );
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(
        result.is_ok(),
        "evaluate should accept GreenComplete state for full workflow, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

/// evaluate() should accept DemoComplete state (needed for full workflow after demo).
#[tokio::test]
async fn evaluate_accepts_demo_complete_state() {
    let session_dir = std::env::temp_dir().join("tddy-demo-eval-accepts-dc");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Summary\nTest.").expect("write PRD");

    common::write_changeset_with_state(&session_dir, "DemoComplete", "sess-demo-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-demo-eval-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_evaluate(
        session_dir.clone(),
        Some(std::path::Path::new(".").to_path_buf()),
    );
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(
        result.is_ok(),
        "evaluate should accept DemoComplete state, got {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// `before_task("demo")` must set `system_prompt` with the `tddy-tools submit --goal demo`
/// contract so agents finish the goal (evaluate/validate set system_prompt the same way).
#[tokio::test]
async fn before_demo_hook_sets_system_prompt_with_submit_contract() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-demo-hook-sys-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("demo-plan.md"), "# Demo\nsteps").expect("write demo-plan.md");

    let mut cs = Changeset::default();
    cs.state.session_id = Some("hook-test-agent-session".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset for resolve_agent_session_id");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());

    hooks.before_task("demo", &ctx).expect("before_task demo");

    let sys: Option<String> = ctx.get_sync("system_prompt");
    let prompt = sys
        .expect("demo goal must set system_prompt on context (mirrors evaluate/validate/refactor)");
    assert!(
        prompt.contains("tddy-tools submit") && prompt.contains("--goal demo"),
        "system_prompt must include tddy-tools submit --goal demo, got {:?}",
        prompt.chars().take(200).collect::<String>()
    );
    assert!(
        prompt.contains("tddy-tools get-schema demo") || prompt.contains("get-schema demo"),
        "system_prompt must reference get-schema for demo goal"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Demo backend invocation should use goal_id "demo".
#[tokio::test]
async fn demo_backend_invocation_uses_goal_demo() {
    let (session_dir, engine, backend) = setup_session_dir_with_green_complete("goal-demo").await;

    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- test\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    backend.push_ok(DEMO_OUTPUT);

    let ctx = ctx_demo(session_dir.clone());
    let result = engine.run_goal(&GoalId::new("demo"), ctx).await;

    assert!(result.is_ok(), "demo should succeed, got {:?}", result);

    let invocations = backend.invocations();
    let demo_inv = invocations.last().expect("should have demo invocation");
    assert_eq!(
        demo_inv.goal_id,
        GoalId::new("demo"),
        "demo invocation should use goal_id demo"
    );

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

/// After `--resume-from`, `before_demo` loads the agent thread id from `changeset.yaml`
/// (`state.session_id`) so the backend uses `SessionMode::Resume` with that id, not the
/// workflow engine storage id.
#[tokio::test]
async fn demo_after_cli_resume_passes_persisted_session_for_agent_resume() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-resume-demo-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    std::fs::write(
        session_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run\n## Verification\nOK",
    )
    .expect("demo-plan");

    let mut cs = Changeset::default();
    cs.state.current = WorkflowState::new("GreenComplete");
    cs.state.session_id = Some("persisted-agent-thread-for-resume".to_string());
    write_changeset(&session_dir, &cs).expect("write changeset");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(DEMO_OUTPUT);
    let storage_dir =
        std::env::temp_dir().join(format!("tddy-resume-demo-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_demo(session_dir.clone());
    let result = engine
        .run_goal(&GoalId::new("demo"), ctx)
        .await
        .expect("demo run");
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "expected Paused after demo, got {:?}",
        result.status
    );

    let inv = backend.invocations();
    let demo_req = inv
        .iter()
        .find(|r| r.goal_id == GoalId::new("demo"))
        .expect("demo invoke");

    assert_eq!(
        demo_req.session.as_ref().map(|s| s.session_id()),
        Some("persisted-agent-thread-for-resume"),
        "InvokeRequest.session must carry changeset state.session_id"
    );
    assert!(
        demo_req
            .session
            .as_ref()
            .is_some_and(|s| s.is_resume()),
        "InvokeRequest.session must be Resume so the agent CLI continues the prior thread; got {:?}",
        demo_req.session
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Changeset should include "demo" in default_models for model resolution.
#[test]
fn changeset_default_models_has_demo_key() {
    let cs = tddy_core::Changeset::default();
    let defaults = tdd_recipe_default_models_str();
    let demo_model = tddy_core::resolve_model(Some(&cs), "demo", None, Some(&defaults));
    assert!(
        demo_model.is_some(),
        "resolve_model for 'demo' should return a model from recipe default_models, got None"
    );
}
