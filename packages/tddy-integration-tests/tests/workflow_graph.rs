//! Workflow graph tests — Milestone 3.
//!
//! Graph-driven tests using StubBackend. No programmatic step calls.
//! Tests: topology, full sequence, conditional edges, WaitForInput,
//! backend errors, parse retry, single-task, session resume.

mod common;

use std::sync::Arc;
use std::sync::Mutex;
use tddy_core::backend::{
    ClarificationQuestion, CodingBackend, MockBackend, QuestionOption, StubBackend,
};
use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::{ExecutionStatus, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{FileSessionStorage, Session, SessionStorage};
use tddy_core::workflow::task::EchoTask;
use tddy_core::workflow::task::TaskResult;
use tddy_core::workflow::task::{BackendInvokeTask, FailingTask, NextAction, Task};
use tddy_core::GoalId;
use tddy_workflow_recipes::tdd::graph::{build_full_tdd_workflow_graph, build_tdd_workflow_graph};
use tddy_workflow_recipes::{
    parse_acceptance_tests_response, parse_green_response, parse_planning_response,
    parse_red_response, parse_update_docs_response, PlanTask, PlanningOutput,
};

use common::stub_invoke_request;

/// Graph topology: build_tdd_workflow_graph creates correct edges.
#[tokio::test]
async fn graph_topology_plan_to_refactor_edges() {
    let backend = Arc::new(StubBackend::new());
    let graph = build_tdd_workflow_graph(backend, common::tdd_recipe());

    assert_eq!(graph.id, "tdd_workflow");

    let ctx = Context::new();
    assert_eq!(
        graph.next_task_id("plan", &ctx),
        Some("acceptance-tests".to_string())
    );
    assert_eq!(
        graph.next_task_id("acceptance-tests", &ctx),
        Some("red".to_string())
    );
    assert_eq!(graph.next_task_id("red", &ctx), Some("green".to_string()));
}

/// Full graph topology: build_full_tdd_workflow_graph includes demo, evaluate, validate, refactor.
#[tokio::test]
async fn full_graph_topology_includes_all_goals() {
    let backend = Arc::new(StubBackend::new());
    let graph = build_full_tdd_workflow_graph(backend, common::tdd_recipe());

    assert_eq!(graph.id, "tdd_full_workflow");

    let task_ids: Vec<String> = graph.task_ids().cloned().collect();
    assert!(
        task_ids.contains(&"plan".to_string()),
        "must have plan task"
    );
    assert!(
        task_ids.contains(&"acceptance-tests".to_string()),
        "must have acceptance-tests"
    );
    assert!(task_ids.contains(&"red".to_string()), "must have red");
    assert!(task_ids.contains(&"green".to_string()), "must have green");
    assert!(task_ids.contains(&"demo".to_string()), "must have demo");
    assert!(
        task_ids.contains(&"evaluate".to_string()),
        "must have evaluate"
    );
    assert!(
        task_ids.contains(&"validate".to_string()),
        "must have validate"
    );
    assert!(
        task_ids.contains(&"refactor".to_string()),
        "must have refactor"
    );
    assert!(
        task_ids.contains(&"update-docs".to_string()),
        "must have update-docs"
    );
    assert!(task_ids.contains(&"end".to_string()), "must have end");
}

/// Full graph: conditional edge from green — run_optional_step_x true -> demo, false -> evaluate.
#[tokio::test]
async fn full_graph_conditional_demo_edge() {
    let backend = Arc::new(StubBackend::new());
    let graph = build_full_tdd_workflow_graph(backend, common::tdd_recipe());

    let ctx_skip = Context::new();
    ctx_skip.set_sync("run_optional_step_x", false);

    let ctx_run = Context::new();
    ctx_run.set_sync("run_optional_step_x", true);

    assert_eq!(
        graph.next_task_id("green", &ctx_skip),
        Some("evaluate".to_string()),
        "run_optional_step_x false -> evaluate"
    );
    assert_eq!(
        graph.next_task_id("green", &ctx_run),
        Some("demo".to_string()),
        "run_optional_step_x true -> demo"
    );

    let ctx_default = Context::new();
    assert_eq!(
        graph.next_task_id("green", &ctx_default),
        Some("evaluate".to_string()),
        "run_optional_step_x unset -> evaluate (default)"
    );

    assert_eq!(
        graph.next_task_id("demo", &ctx_default),
        Some("evaluate".to_string())
    );
    assert_eq!(
        graph.next_task_id("evaluate", &ctx_default),
        Some("validate".to_string())
    );
    assert_eq!(
        graph.next_task_id("validate", &ctx_default),
        Some("refactor".to_string())
    );
    assert_eq!(
        graph.next_task_id("refactor", &ctx_default),
        Some("update-docs".to_string())
    );
    assert_eq!(
        graph.next_task_id("update-docs", &ctx_default),
        Some("end".to_string())
    );
}

/// StubBackend returns valid plan output via tool executor (parseable from take_submit_result).
/// Uses SKIP_QUESTIONS to bypass clarification and get the plan directly.
#[tokio::test]
async fn stub_backend_plan_returns_valid_structured_response() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("SKIP_QUESTIONS Add user auth", "plan");

    let _resp = backend.invoke(req).await.unwrap();
    let ch = backend.submit_channel().expect("StubBackend has channel");
    let output = ch
        .take_for_goal("plan")
        .expect("stub stores plan via tool executor");
    let parsed = parse_planning_response(&output).expect("should parse plan");
    assert!(!parsed.prd.is_empty());
}

/// StubBackend returns valid acceptance-tests output via tool executor.
#[tokio::test]
async fn stub_backend_acceptance_tests_returns_valid_response() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("HERE ARE THE USER'S ANSWERS\n\nYes", "acceptance-tests");

    let _resp = backend.invoke(req).await.unwrap();
    let ch = backend.submit_channel().expect("StubBackend has channel");
    let output = ch
        .take_for_goal("acceptance-tests")
        .expect("stub stores acceptance-tests via tool executor");
    let parsed = parse_acceptance_tests_response(&output).expect("should parse");
    assert!(!parsed.summary.is_empty());
    assert!(!parsed.tests.is_empty());
}

/// StubBackend returns valid red output via tool executor.
#[tokio::test]
async fn stub_backend_red_returns_valid_response() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("red", "red");

    let _resp = backend.invoke(req).await.unwrap();
    let ch = backend.submit_channel().expect("StubBackend has channel");
    let output = ch
        .take_for_goal("red")
        .expect("stub stores red via tool executor");
    let parsed = parse_red_response(&output).expect("should parse");
    assert!(!parsed.summary.is_empty());
}

/// StubBackend returns valid update-docs output.
#[tokio::test]
async fn stub_backend_update_docs_returns_valid_response() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("update docs", "update-docs");

    let _resp = backend.invoke(req).await.unwrap();
    let ch = backend.submit_channel().expect("StubBackend has channel");
    let output = ch
        .take_for_goal("update-docs")
        .expect("stub stores update-docs via tool executor");
    let parsed = parse_update_docs_response(&output).expect("should parse");
    assert!(!parsed.summary.is_empty());
    assert_eq!(parsed.docs_updated, 3);
}

/// StubBackend returns valid green output via tool executor.
#[tokio::test]
async fn stub_backend_green_returns_valid_response() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("green", "green");

    let _resp = backend.invoke(req).await.unwrap();
    let ch = backend.submit_channel().expect("StubBackend has channel");
    let output = ch
        .take_for_goal("green")
        .expect("stub stores green via tool executor");
    let parsed = parse_green_response(&output).expect("should parse");
    assert!(!parsed.summary.is_empty());
}

/// CLARIFY with "Here are the user's answers" in prompt skips clarification (returns normal response).
#[tokio::test]
async fn stub_backend_clarify_with_answers_skips_questions() {
    let backend = StubBackend::new();
    let req = stub_invoke_request(
        "Here are the user's answers to your questions:\n\nEmail/password\n\nNow create the PRD for: CLARIFY test",
        "plan",
    );

    let resp = backend.invoke(req).await.unwrap();
    assert!(
        resp.questions.is_empty(),
        "stub should skip clarification when answers in prompt"
    );
}

/// CLARIFY magic word returns clarification questions.
#[tokio::test]
async fn stub_backend_clarify_returns_questions() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("Add auth CLARIFY", "plan");

    let resp = backend.invoke(req).await.unwrap();
    assert!(!resp.questions.is_empty());
}

/// When backend returns both submit result and stream-extracted questions (e.g. from failed
/// AskUserQuestion retries), submit takes priority — task proceeds with Continue, not WaitForInput.
#[tokio::test]
async fn backend_invoke_task_submit_takes_priority_over_questions() {
    let plan_json = r##"{"goal":"plan","prd":"# PRD\n\n## TODO\n\n- [ ] Task","todo_items":[{"id":"1","title":"Task","done":false}]}"##;
    let stale_questions = vec![ClarificationQuestion {
        header: "Early errors".to_string(),
        question: "When an error occurs, should Resume be offered?".to_string(),
        options: vec![
            QuestionOption {
                label: "Context-aware".to_string(),
                description: "Only when session exists".to_string(),
            },
            QuestionOption {
                label: "Always".to_string(),
                description: "Always show Resume".to_string(),
            },
        ],
        multi_select: false,
        allow_other: true,
    }];

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_questions(plan_json, "sess-1", stale_questions);
    // Pre-populate submit channel to simulate tddy-tools submit (agent submitted despite stale questions)
    backend
        .as_ref()
        .submit_channel()
        .expect("MockBackend has channel")
        .store("plan", plan_json);

    let task = BackendInvokeTask::from_recipe(
        "plan",
        GoalId::new("plan"),
        common::tdd_recipe().as_ref(),
        backend.clone(),
    );
    let ctx = Context::new();
    ctx.set_sync("prompt", "Add feature");
    ctx.set_sync("output_dir", std::path::PathBuf::from("/tmp"));

    let result = task.run(ctx).await.expect("task should succeed");

    assert_eq!(
        result.next_action,
        NextAction::Continue,
        "submit should take priority over stale questions"
    );
    assert!(
        result.response.contains("PRD"),
        "response should be the submitted plan, not clarification"
    );
}

/// FAIL_INVOKE magic word returns BackendError.
#[tokio::test]
async fn stub_backend_fail_invoke_returns_error() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("Add auth FAIL_INVOKE", "plan");

    let result = backend.invoke(req).await;
    assert!(result.is_err());
}

/// FAIL_PARSE magic word returns malformed structured response.
#[tokio::test]
async fn stub_backend_fail_parse_returns_malformed() {
    let backend = StubBackend::new();
    let req = stub_invoke_request("Add auth FAIL_PARSE", "plan");

    let resp = backend.invoke(req).await.unwrap();
    let result = parse_planning_response(&resp.output);
    assert!(result.is_err());
}

/// Single-task execution: FlowRunner runs one task.
#[tokio::test]
async fn flow_runner_single_task_execution() {
    let dir = std::env::temp_dir().join("tddy-workflow-single");
    let _ = std::fs::remove_dir_all(&dir);
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let task = Arc::new(EchoTask::new("echo"));
    let graph = Arc::new(
        GraphBuilder::new("single")
            .add_task(task)
            .add_edge("echo", "echo")
            .build(),
    );

    let session =
        Session::new_from_task("run1".to_string(), "single".to_string(), "echo".to_string());
    session.context.set_sync("input", "single task test");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new(graph, storage.clone());
    let result = runner.run("run1").await.unwrap();

    assert_eq!(result.session_id, "run1");
    assert!(matches!(
        result.status,
        ExecutionStatus::Paused { .. } | ExecutionStatus::Completed
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Session resume: save mid-workflow, load with new FlowRunner, resume.
#[tokio::test]
async fn session_resume_after_save() {
    let dir = std::env::temp_dir().join("tddy-workflow-resume");
    let _ = std::fs::remove_dir_all(&dir);
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let graph = Arc::new(
        GraphBuilder::new("resume")
            .add_task(t1)
            .add_task(t2)
            .add_edge("t1", "t2")
            .add_edge("t2", "t2")
            .build(),
    );

    let session =
        Session::new_from_task("res1".to_string(), "resume".to_string(), "t1".to_string());
    session.context.set_sync("input", "first");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new(graph.clone(), storage.clone());
    let r1 = runner.run("res1").await.unwrap();
    assert_eq!(r1.current_task_id, Some("t2".to_string()));

    // New runner, same storage — should resume from t2
    let runner2 = FlowRunner::new(graph, storage.clone());
    let r2 = runner2.run("res1").await.unwrap();
    assert_eq!(r2.current_task_id, Some("t2".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Full step Tasks (PlanTask, RedTask, GreenTask) ───────────────────────────

/// PlanTask run writes parsed_planning to context when given feature_input and output_dir.
#[tokio::test]
async fn plan_task_run_writes_parsed_planning_to_context() {
    let output_dir = std::env::temp_dir().join("tddy-plan-task-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let backend = Arc::new(StubBackend::new());
    let task = PlanTask::new(backend, common::tdd_recipe());

    let ctx = Context::new();
    ctx.set_sync("feature_input", "Add user auth SKIP_QUESTIONS");
    ctx.set_sync("output_dir", output_dir.clone());

    let result = task
        .run(ctx.clone())
        .await
        .expect("PlanTask should succeed");
    assert_eq!(result.task_id, "plan");

    let planning: Option<PlanningOutput> = ctx.get_sync("parsed_planning");
    assert!(planning.is_some(), "parsed_planning should be in context");
    let planning = planning.unwrap();
    assert!(!planning.prd.is_empty());

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// FlowRunner drives full TDD sequence (plan -> acceptance-tests -> red -> green -> end) with StubBackend.
#[tokio::test]
async fn flow_runner_tdd_full_sequence_completes() {
    use tddy_core::workflow::graph::ExecutionStatus;

    let dir = std::env::temp_dir().join("tddy-flowrunner-full-seq");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let backend = Arc::new(StubBackend::new());
    let graph = Arc::new(build_tdd_workflow_graph(backend, common::tdd_recipe()));

    let session_dir = dir.join("plan");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    let init_cs = Changeset {
        initial_prompt: Some("Add a feature SKIP_QUESTIONS".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);
    let session = Session::new_from_task(
        "full1".to_string(),
        "tdd_workflow".to_string(),
        "plan".to_string(),
    );
    session
        .context
        .set_sync("feature_input", "Add a feature SKIP_QUESTIONS");
    session.context.set_sync("output_dir", session_dir);
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new(graph, storage.clone());

    let mut result = runner.run("full1").await.unwrap();
    while !matches!(result.status, ExecutionStatus::Completed) {
        if matches!(result.status, ExecutionStatus::WaitingForInput { .. }) {
            panic!("FlowRunner should not block on WaitForInput (use SKIP_QUESTIONS in prompt)");
        }
        result = runner.run("full1").await.unwrap();
    }

    assert_eq!(result.session_id, "full1");
    assert_eq!(result.current_task_id, None);

    let _ = std::fs::remove_dir_all(&dir);
}

// ── RunnerHooks tests ────────────────────────────────────────────────────────

/// Hooks implementation that records call order for testing.
struct RecordHooks {
    calls: Mutex<Vec<String>>,
}

impl RecordHooks {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn take_calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().drain(..).collect()
    }
}

impl RunnerHooks for RecordHooks {
    fn before_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("before:{}", task_id));
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("after:{}", task_id));
        Ok(())
    }

    fn on_error(
        &self,
        task_id: &str,
        _context: &tddy_core::workflow::context::Context,
        _error: &(dyn std::error::Error + Send + Sync),
    ) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("error:{}", task_id));
    }
}

/// Hooks that trigger ElicitationNeeded for a specific task_id.
struct ElicitationHooks {
    trigger_task: String,
    calls: Mutex<Vec<String>>,
}

impl ElicitationHooks {
    fn new(trigger_task: &str) -> Self {
        Self {
            trigger_task: trigger_task.to_string(),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl RunnerHooks for ElicitationHooks {
    fn before_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("before:{}", task_id));
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("after:{}", task_id));
        Ok(())
    }

    fn elicitation_after_task(
        &self,
        task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Option<tddy_core::workflow::graph::ElicitationEvent> {
        if task_id == self.trigger_task {
            self.calls
                .lock()
                .unwrap()
                .push(format!("elicitation:{}", task_id));
            Some(
                tddy_core::workflow::graph::ElicitationEvent::DocumentApproval {
                    content: "# Test PRD".to_string(),
                },
            )
        } else {
            None
        }
    }

    fn on_error(
        &self,
        task_id: &str,
        _context: &tddy_core::workflow::context::Context,
        _error: &(dyn std::error::Error + Send + Sync),
    ) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("error:{}", task_id));
    }
}

/// Hooks that fail in before_task.
struct FailBeforeHooks;

impl RunnerHooks for FailBeforeHooks {
    fn before_task(
        &self,
        _task_id: &str,
        _context: &Context,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("before_task failed".into())
    }

    fn after_task(
        &self,
        _task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    fn on_error(
        &self,
        _task_id: &str,
        _context: &tddy_core::workflow::context::Context,
        _error: &(dyn std::error::Error + Send + Sync),
    ) {
    }
}

/// FlowRunner with hooks: before_task and after_task called in correct order.
#[tokio::test]
async fn flow_runner_hooks_called_in_order() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-hooks");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let hooks = Arc::new(RecordHooks::new());
    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let graph = Arc::new(
        GraphBuilder::new("hooks_test")
            .add_task(t1)
            .add_task(t2)
            .add_edge("t1", "t2")
            .add_edge("t2", "t2")
            .build(),
    );

    let session =
        Session::new_from_task("h1".to_string(), "hooks_test".to_string(), "t1".to_string());
    session.context.set_sync("input", "test");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks.clone()));
    let _ = runner.run("h1").await.unwrap();

    let calls = hooks.take_calls();
    assert_eq!(calls, vec!["before:t1", "after:t1"]);

    let _ = runner.run("h1").await.unwrap();
    let calls = hooks.take_calls();
    assert_eq!(calls, vec!["before:t2", "after:t2"]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// FlowRunner with None hooks: works without hooks (backward compat).
#[tokio::test]
async fn flow_runner_none_hooks_works() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-none-hooks");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let task = Arc::new(EchoTask::new("echo"));
    let graph = Arc::new(
        GraphBuilder::new("none_hooks")
            .add_task(task)
            .add_edge("echo", "echo")
            .build(),
    );

    let session = Session::new_from_task(
        "n1".to_string(),
        "none_hooks".to_string(),
        "echo".to_string(),
    );
    session.context.set_sync("input", "no hooks");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new(graph, storage.clone());
    let result = runner.run("n1").await.unwrap();

    assert_eq!(result.session_id, "n1");
    assert!(matches!(
        result.status,
        ExecutionStatus::Paused { .. } | ExecutionStatus::Completed
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

/// FlowRunner: before_task error propagates.
#[tokio::test]
async fn flow_runner_before_task_error_propagates() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-before-err");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let hooks = Arc::new(FailBeforeHooks);
    let task = Arc::new(EchoTask::new("echo"));
    let graph = Arc::new(
        GraphBuilder::new("fail_before")
            .add_task(task)
            .add_edge("echo", "echo")
            .build(),
    );

    let session = Session::new_from_task(
        "f1".to_string(),
        "fail_before".to_string(),
        "echo".to_string(),
    );
    session.context.set_sync("input", "test");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));
    let result = runner.run("f1").await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("before_task failed"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// FlowRunner: on_error called when task fails.
#[tokio::test]
async fn flow_runner_on_error_called_when_task_fails() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-on-error");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let hooks = Arc::new(RecordHooks::new());
    let task = Arc::new(FailingTask::new("fail"));
    let graph = Arc::new(
        GraphBuilder::new("on_error_test")
            .add_task(task)
            .add_edge("fail", "fail")
            .build(),
    );

    let session = Session::new_from_task(
        "e1".to_string(),
        "on_error_test".to_string(),
        "fail".to_string(),
    );
    session.context.set_sync("input", "ignored");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks.clone()));
    let result = runner.run("e1").await;

    assert!(result.is_err());
    let calls = hooks.take_calls();
    assert_eq!(calls, vec!["before:fail", "error:fail"]);

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Hook-Triggered Elicitation tests ─────────────────────────────────────────

/// elicitation_after_task default returns None (trait default).
#[tokio::test]
async fn elicitation_after_task_default_returns_none() {
    let hooks = RecordHooks::new();
    let ctx = Context::new();
    let result = TaskResult {
        task_id: "plan".to_string(),
        response: "test".to_string(),
        next_action: tddy_core::workflow::task::NextAction::Continue,
        status_message: None,
    };
    assert!(
        hooks
            .elicitation_after_task("plan", &ctx, &result)
            .is_none(),
        "default implementation should return None"
    );
}

/// FlowRunner returns ElicitationNeeded when hook signals elicitation.
#[tokio::test]
async fn flow_runner_returns_elicitation_needed_when_hook_signals() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-elicitation");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let hooks = Arc::new(ElicitationHooks::new("t1"));
    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let graph = Arc::new(
        GraphBuilder::new("elicit_test")
            .add_task(t1)
            .add_task(t2)
            .add_edge("t1", "t2")
            .add_edge("t2", "t2")
            .build(),
    );

    let session = Session::new_from_task(
        "el1".to_string(),
        "elicit_test".to_string(),
        "t1".to_string(),
    );
    session.context.set_sync("input", "test");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));
    let result = runner.run("el1").await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "should return ElicitationNeeded, got {:?}",
        result.status
    );

    if let ExecutionStatus::ElicitationNeeded { ref event } = result.status {
        match event {
            tddy_core::workflow::graph::ElicitationEvent::DocumentApproval { ref content } => {
                assert_eq!(content, "# Test PRD");
            }
            tddy_core::workflow::graph::ElicitationEvent::WorktreeConfirmation { .. } => {
                panic!("test expects DocumentApproval, not WorktreeConfirmation");
            }
        }
    }

    assert_eq!(
        result.current_task_id,
        Some("t2".to_string()),
        "session should advance to next task despite ElicitationNeeded"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// FlowRunner returns Paused when hook returns None (no elicitation).
#[tokio::test]
async fn flow_runner_returns_paused_when_no_elicitation() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-no-elicit");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let hooks = Arc::new(ElicitationHooks::new("other_task"));
    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let graph = Arc::new(
        GraphBuilder::new("no_elicit")
            .add_task(t1)
            .add_task(t2)
            .add_edge("t1", "t2")
            .add_edge("t2", "t2")
            .build(),
    );

    let session =
        Session::new_from_task("ne1".to_string(), "no_elicit".to_string(), "t1".to_string());
    session.context.set_sync("input", "test");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));
    let result = runner.run("ne1").await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "should return Paused when elicitation hook returns None, got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// WorkflowEngine returns ElicitationNeeded to caller (does not auto-continue).
#[tokio::test]
async fn engine_returns_elicitation_needed_to_caller() {
    use tddy_core::WorkflowEngine;

    let dir = std::env::temp_dir().join("tddy-engine-elicitation");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let backend = Arc::new(StubBackend::new());
    let session_dir = dir.join("plan");
    std::fs::create_dir_all(&session_dir).unwrap();
    let init_cs = Changeset {
        initial_prompt: Some("Build auth SKIP_QUESTIONS".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let storage_dir = std::env::temp_dir().join("tddy-engine-elicit-storage");
    let _ = std::fs::remove_dir_all(&storage_dir);

    let hooks = Arc::new(ElicitationHooks::new("plan"));
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        tddy_core::SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth SKIP_QUESTIONS"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(dir.clone()).unwrap(),
    );
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );

    let plan_gid = GoalId::new("plan");
    let result = engine.run_goal(&plan_gid, ctx).await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "engine should return ElicitationNeeded to caller, got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

/// WorkflowEngine run_workflow_from also returns ElicitationNeeded to caller.
#[tokio::test]
async fn engine_run_workflow_from_returns_elicitation_needed() {
    use tddy_core::WorkflowEngine;

    let dir = std::env::temp_dir().join("tddy-engine-wf-elicitation");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let backend = Arc::new(StubBackend::new());
    let session_dir = dir.join("plan");
    std::fs::create_dir_all(&session_dir).unwrap();
    let init_cs = Changeset {
        initial_prompt: Some("Build auth SKIP_QUESTIONS".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let storage_dir = std::env::temp_dir().join("tddy-engine-wf-elicit-storage");
    let _ = std::fs::remove_dir_all(&storage_dir);

    let hooks = Arc::new(ElicitationHooks::new("plan"));
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        tddy_core::SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth SKIP_QUESTIONS"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(dir.clone()).unwrap(),
    );
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir).unwrap(),
    );

    let plan_gid = GoalId::new("plan");
    let result = engine.run_workflow_from(&plan_gid, ctx).await.unwrap();

    assert!(
        matches!(result.status, ExecutionStatus::ElicitationNeeded { .. }),
        "run_workflow_from should return ElicitationNeeded, got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}
