//! Integration tests for the evaluate-changes workflow with MockBackend.
//!
//! All tests reference types and methods introduced by the evaluate-changes rename:
//! - `Goal::Evaluate` (renamed from Goal::Validate)
//! - `EvaluateOptions` (renamed from ValidateOptions)
//! - `workflow.evaluate()` -> run_goal_until_done with ctx_evaluate
//! - `evaluate_allowlist()` for the evaluate goal
//! - `evaluation-report.md` written to session_dir (not working_dir)
//! - New report fields: changed_files, affected_tests, validity_assessment
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::{GoalId, MockBackend, SharedBackend};
use tddy_workflow_recipes::evaluate_allowlist;
use tddy_workflow_recipes::TddWorkflowHooks;

use common::{ctx_evaluate, run_goal_until_done, write_changeset_with_state};

/// Full evaluate-changes output as JSON (tddy-tools submit format).
const EVALUATE_OUTPUT: &str = r#"{"goal":"evaluate-changes","summary":"Evaluated 3 changed files. Risk level: medium. Found 2 issues.","risk_level":"medium","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[{"severity":"warning","category":"code_quality","file":"src/main.rs","line":42,"description":"Function exceeds 50 lines","suggestion":"Extract into smaller functions"},{"severity":"info","category":"test_infrastructure","file":"src/lib.rs","line":10,"description":"Test helper visible in production","suggestion":"Move to test module"}],"changeset_sync":{"status":"not_found","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/main.rs","lines_changed":25,"changeset_item":null}],"test_impact":{"tests_affected":2,"new_tests_needed":1},"changed_files":[{"path":"src/main.rs","change_type":"modified","lines_added":15,"lines_removed":3},{"path":"src/lib.rs","change_type":"modified","lines_added":5,"lines_removed":0},{"path":"tests/main_test.rs","change_type":"added","lines_added":40,"lines_removed":0}],"affected_tests":[{"path":"tests/main_test.rs","status":"created","description":"New acceptance tests for the main module"},{"path":"tests/integration_test.rs","status":"updated","description":"Updated to cover new code paths"}],"validity_assessment":"The change is valid for the intended use-case. All acceptance criteria from the PRD are addressed. The new code follows existing patterns and does not introduce breaking changes. Risk is medium due to the size of the diff."}"#;

/// For run_goal_until_done(evaluate): evaluate -> validate -> refactor chain.
const VALIDATE_OUTPUT: &str = r#"{"goal":"validate","summary":"All 3 subagents completed.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;

const REFACTOR_OUTPUT: &str = r#"{"goal":"refactor","summary":"Completed. All tests passing.","tasks_completed":5,"tests_passing":true}"#;

const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

/// evaluate() invokes backend with Goal::Evaluate (renamed from Goal::Validate).
#[tokio::test]
async fn evaluate_workflow_invokes_backend_with_evaluate_goal() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-goal-test");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-goal-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-goal");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend.clone()), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let eval_gid = GoalId::new("evaluate");
    let result = engine.run_goal(&eval_gid, ctx).await;

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal_id.as_str(),
        "evaluate",
        "InvokeRequest should have goal evaluate (graph task id)"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() transitions workflow to Evaluated state on success.
#[tokio::test]
async fn evaluate_workflow_transitions_to_evaluated_state() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-state-test");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-state-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let eval_gid = GoalId::new("evaluate");
    let _ = engine.run_goal(&eval_gid, ctx).await.unwrap();

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_eq!(
        changeset.state.current,
        WorkflowState::new("Evaluated"),
        "workflow should transition to Evaluated (not Validated), got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() writes evaluation-report.md to session_dir, NOT to working_dir.
#[tokio::test]
async fn evaluate_workflow_writes_evaluation_report_to_session_dir() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-writes-working");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-writes-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-writes");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-writes-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let _ = run_goal_until_done(&engine, "evaluate", ctx).await.unwrap();

    let report_in_plan = session_dir.join("evaluation-report.md");
    assert!(
        report_in_plan.exists(),
        "evaluation-report.md must be written to session_dir, not found at: {}",
        report_in_plan.display()
    );

    let report_in_working = working_dir.join("evaluation-report.md");
    assert!(
        !report_in_working.exists(),
        "evaluation-report.md must NOT be in working_dir — it belongs in session_dir, found at: {}",
        report_in_working.display()
    );

    assert!(
        !session_dir.join("validation-report.md").exists(),
        "old validation-report.md must not be created; the new name is evaluation-report.md"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() without session_dir returns an error (session_dir is required).
#[tokio::test]
async fn evaluate_workflow_requires_session_dir() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-no-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-no-plan-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = std::collections::HashMap::new();
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(
        result.is_err(),
        "evaluate should fail when session_dir is None — session_dir is required for evaluate-changes"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
}

/// evaluate() report includes a Changed Files section listing all changed files.
#[tokio::test]
async fn evaluate_workflow_includes_changed_files_in_report() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-changed-files");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-changed-files-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-changed-files");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-changed-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(session_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in session_dir");

    assert!(
        report.contains("Changed Files") || report.contains("changed_files") || report.contains("src/main.rs"),
        "evaluation-report.md must include a Changed Files section listing all modified files, got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() report includes an Affected Tests section.
#[tokio::test]
async fn evaluate_workflow_includes_affected_tests_in_report() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-affected-tests");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-affected-tests-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-affected-tests");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-affected-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(session_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in session_dir");

    assert!(
        report.contains("Affected Tests")
            || report.contains("affected_tests")
            || report.contains("main_test.rs"),
        "evaluation-report.md must include an Affected Tests section (created/updated/removed/skipped), got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() report includes a Validity Assessment section.
#[tokio::test]
async fn evaluate_workflow_includes_validity_assessment() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-validity");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-validity-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-validity");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);
    backend.push_ok(VALIDATE_OUTPUT);
    backend.push_ok(REFACTOR_OUTPUT);
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-validity-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(session_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in session_dir");

    assert!(
        report.contains("Validity Assessment")
            || report.contains("validity_assessment")
            || report.contains("valid for the intended use-case"),
        "evaluation-report.md must include a Validity Assessment section with detailed analysis, got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() includes PRD/changeset context in prompt when session_dir is provided.
#[tokio::test]
async fn evaluate_workflow_includes_session_dir_context_when_provided() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-with-plan-dir");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-plan-context");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");

    std::fs::write(
        session_dir.join("PRD.md"),
        "# PRD\n## Summary\nAuth feature.",
    )
    .expect("write PRD");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-ctx-456");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(EVALUATE_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-ctx-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend.clone()), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let eval_gid = GoalId::new("evaluate");
    let result = engine.run_goal(&eval_gid, ctx).await;

    assert!(
        result.is_ok(),
        "evaluate with session_dir should succeed, got: {:?}",
        result
    );

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert!(
        req.prompt.contains("Auth feature")
            || req.prompt.contains("PRD")
            || req.prompt.contains("changeset"),
        "prompt must include plan context (PRD/changeset) when session_dir is provided, got prompt start: {}",
        &req.prompt[..req.prompt.len().min(200)]
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate_allowlist() contains required read, git and cargo tools.
#[test]
fn evaluate_allowlist_contains_required_tools() {
    let allowlist = evaluate_allowlist();

    assert!(
        allowlist.iter().any(|t| t == "Read"),
        "evaluate_allowlist must include Read, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Glob"),
        "evaluate_allowlist must include Glob, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Grep"),
        "evaluate_allowlist must include Grep, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t.contains("git diff")),
        "evaluate_allowlist must include a Bash(git diff *) entry, got: {:?}",
        allowlist
    );
    assert!(
        allowlist
            .iter()
            .any(|t| t.contains("cargo build") || t.contains("cargo check")),
        "evaluate_allowlist must include a Bash(cargo build *) or Bash(cargo check *) entry, got: {:?}",
        allowlist
    );
}

/// When the agent finishes without a delivered `tddy-tools submit` (e.g. repeated validation
/// failure before relay), the workflow fails with an explicit error instead of hanging.
#[tokio::test]
async fn evaluate_workflow_fails_when_agent_finished_without_submit() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-no-submit");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-no-submit-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-eval-no-submit");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_without_submit("Agent finished; submit never relayed.");

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-no-submit-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(
        result.is_err(),
        "evaluate should fail when no submit was delivered, got: {:?}",
        result
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("without calling tddy-tools submit")
            || err_msg.contains("evaluate-changes"),
        "expected missing-submit workflow error, got: {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// evaluate() returns ParseError when backend returns a response with no structured-response block.
#[tokio::test]
async fn evaluate_workflow_returns_parse_error_on_malformed_response() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-parse-error");
    let session_dir = std::env::temp_dir().join("tddy-evaluate-parse-error-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok("I evaluated the changes and they look fine. No issues found.");

    let storage_dir = std::env::temp_dir().join("tddy-evaluate-parse-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let ctx = ctx_evaluate(session_dir.clone(), Some(working_dir.clone()));
    let result = run_goal_until_done(&engine, "evaluate", ctx).await;

    assert!(
        result.is_err(),
        "evaluate should fail on malformed response (missing structured-response block)"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Parse")
            || err_msg.contains("parse")
            || err_msg.contains("malformed")
            || err_msg.contains("JSON"),
        "expected ParseError or malformed, got: {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&session_dir);
}

/// When the workflow enters the evaluate goal, `changeset.yaml` must record the real phase
/// (`Evaluating`), not the previous completed state (`GreenComplete`). Otherwise resume logic
/// (`next_goal_for_state` in `run_workflow`) maps `GreenComplete` → `demo` and the session
/// continues at the wrong goal instead of evaluate.
#[tokio::test]
async fn entering_evaluate_persists_evaluating_in_changeset_for_resume() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-evaluate-persist-state-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-resume-eval");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Summary\nTest.").expect("write PRD");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks
        .before_task("evaluate", &ctx)
        .expect("before_task evaluate");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("Evaluating"),
        "changeset must move to Evaluating when the evaluate goal starts so resume does not treat the workflow as still at GreenComplete (which maps to demo, not evaluate)"
    );
    assert_eq!(
        common::tdd_recipe().next_goal_for_state(&cs.state.current),
        Some(GoalId::new("evaluate")),
        "resume start_goal must be evaluate while evaluation is in progress"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
