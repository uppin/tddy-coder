//! Acceptance tests for Milestone 1: graph-flow-compatible traits.
//!
//! These tests define the expected behavior of Task, Context, Graph, SessionStorage, and FlowRunner.
//! They verify the workflow engine can be used end-to-end.

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use tddy_core::backend::{MockBackend, StubBackend};
use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::{ExecutionStatus, GraphBuilder};
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{FileSessionStorage, Session, SessionStorage};
use tddy_core::workflow::task::{EchoTask, NextAction, Task};
use tddy_core::GoalId;
use tddy_core::{AnyBackend, SharedBackend};
use tddy_workflow_recipes::tdd::graph::build_tdd_workflow_graph;

#[tokio::test]
async fn context_stores_and_retrieves_values() {
    let ctx = Context::new();
    ctx.set_sync("key", "value");
    let v: Option<String> = ctx.get_sync("key");
    assert_eq!(v, Some("value".to_string()));

    ctx.set_sync("num", 42_i32);
    let n: Option<i32> = ctx.get_sync("num");
    assert_eq!(n, Some(42));
}

#[tokio::test]
async fn context_async_get_set() {
    let ctx = Context::new();
    ctx.set("async_key", "async_value").await;
    let v: Option<String> = ctx.get("async_key").await;
    assert_eq!(v, Some("async_value".to_string()));
}

#[tokio::test]
async fn next_action_variants_exist() {
    let _ = NextAction::Continue;
    let _ = NextAction::ContinueAndExecute;
    let _ = NextAction::WaitForInput;
    let _ = NextAction::End;
    let _ = NextAction::GoTo("task_id".to_string());
    let _ = NextAction::GoBack;
}

#[tokio::test]
async fn echo_task_runs_and_returns_result() {
    let task = EchoTask::new("echo");
    let ctx = Context::new();
    ctx.set_sync("input", "hello");

    let result = task.run(ctx).await.unwrap();
    assert_eq!(result.response, "hello");
    assert_eq!(result.task_id, "echo");
    assert!(matches!(result.next_action, NextAction::Continue));
}

#[tokio::test]
async fn graph_builder_creates_graph_with_edges() {
    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));

    let graph = GraphBuilder::new("test_graph")
        .add_task(t1.clone())
        .add_task(t2.clone())
        .add_edge("t1", "t2")
        .build();

    assert_eq!(graph.id, "test_graph");
    assert!(graph.get_task("t1").is_some());
    assert!(graph.get_task("t2").is_some());

    let ctx = Context::new();
    let next = graph.next_task_id("t1", &ctx);
    assert_eq!(next, Some("t2".to_string()));
}

#[tokio::test]
async fn graph_builder_supports_conditional_edges() {
    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let t3 = Arc::new(EchoTask::new("t3"));

    let graph = GraphBuilder::new("cond_graph")
        .add_task(t1.clone())
        .add_task(t2.clone())
        .add_task(t3.clone())
        .add_conditional_edge(
            "t1",
            |ctx| ctx.get_sync::<bool>("use_t2").unwrap_or(false),
            "t2",
            "t3",
        )
        .build();

    let ctx = Context::new();
    ctx.set_sync("use_t2", true);
    assert_eq!(graph.next_task_id("t1", &ctx), Some("t2".to_string()));

    ctx.set_sync("use_t2", false);
    assert_eq!(graph.next_task_id("t1", &ctx), Some("t3".to_string()));
}

#[tokio::test]
async fn session_storage_saves_and_loads() {
    let dir = std::env::temp_dir().join("tddy-session-test");
    let _ = std::fs::remove_dir_all(&dir);
    let storage = FileSessionStorage::new(dir.clone());

    let session = Session::new_from_task("s1".to_string(), "g1".to_string(), "start".to_string());
    session.context.set_sync("foo", "bar");

    storage.save(&session).await.unwrap();

    let loaded = storage
        .get("s1")
        .await
        .unwrap()
        .expect("session should exist");
    assert_eq!(loaded.id, "s1");
    assert_eq!(loaded.current_task_id, "start");
    let v: Option<String> = loaded.context.get_sync("foo");
    assert_eq!(v, Some("bar".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn flow_runner_executes_single_task() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-test");
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
    session.context.set_sync("input", "flow runner test");
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

#[tokio::test]
async fn flow_runner_chains_two_tasks() {
    let dir = std::env::temp_dir().join("tddy-flowrunner-chain");
    let _ = std::fs::remove_dir_all(&dir);
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let t1 = Arc::new(EchoTask::new("t1"));
    let t2 = Arc::new(EchoTask::new("t2"));
    let graph = Arc::new(
        GraphBuilder::new("chain")
            .add_task(t1)
            .add_task(t2)
            .add_edge("t1", "t2")
            .add_edge("t2", "t2")
            .build(),
    );

    let session =
        Session::new_from_task("chain1".to_string(), "chain".to_string(), "t1".to_string());
    session.context.set_sync("input", "first");
    storage.save(&session).await.unwrap();

    let runner = FlowRunner::new(graph, storage.clone());

    let r1 = runner.run("chain1").await.unwrap();
    assert_eq!(r1.current_task_id, Some("t2".to_string()));

    let r2 = runner.run("chain1").await.unwrap();
    assert_eq!(r2.current_task_id, Some("t2".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn build_tdd_workflow_graph_creates_graph() {
    let backend = Arc::new(MockBackend::new());
    let graph = build_tdd_workflow_graph(backend.clone(), common::tdd_recipe());

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

#[tokio::test]
async fn workflow_engine_run_goal_plan_completes() {
    let storage_dir = std::env::temp_dir().join("tddy-engine-plan-test");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let output_dir = storage_dir.join("plan-output");
    std::fs::create_dir_all(&output_dir).unwrap();
    let session_dir = common::session_dir_for_new_session();
    std::fs::create_dir_all(&session_dir).unwrap();
    let init_cs = Changeset {
        initial_prompt: Some("SKIP_QUESTIONS feature".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let backend: SharedBackend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let engine = common::tdd_engine(backend.clone(), storage_dir.clone());

    let mut context_values = HashMap::new();
    context_values.insert(
        "feature_input".to_string(),
        serde_json::json!("SKIP_QUESTIONS feature"),
    );
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.clone()).unwrap(),
    );
    context_values.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );

    let plan_gid = GoalId::new("plan");
    let result = engine.run_goal(&plan_gid, context_values).await.unwrap();

    assert_eq!(result.session_id.len(), 36);
    assert!(
        matches!(
            result.status,
            ExecutionStatus::Paused { .. }
                | ExecutionStatus::Completed
                | ExecutionStatus::ElicitationNeeded { .. }
        ),
        "plan should return Paused, Completed, or ElicitationNeeded; got {:?}",
        result.status
    );
    let prd_path = session_dir.join("artifacts").join("PRD.md");
    assert!(prd_path.exists(), "expected {:?}", prd_path);
    let prd = std::fs::read_to_string(&prd_path).unwrap();
    assert!(
        prd.contains("- [ ]") || prd.contains("## TODO"),
        "PRD.md should contain TODO content (merged as last section)"
    );

    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn workflow_engine_run_full_workflow_completes_with_stub() {
    let storage_dir = std::env::temp_dir().join("tddy-engine-full-test");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let output_dir = storage_dir.join("plan-output");
    std::fs::create_dir_all(&output_dir).unwrap();
    let session_dir = common::session_dir_for_new_session();
    std::fs::create_dir_all(&session_dir).unwrap();
    let init_cs = Changeset {
        initial_prompt: Some("SKIP_QUESTIONS feature".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let backend: SharedBackend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let engine = common::tdd_engine(backend.clone(), storage_dir.clone());

    let mut context_values = HashMap::new();
    context_values.insert(
        "feature_input".to_string(),
        serde_json::json!("SKIP_QUESTIONS feature"),
    );
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.clone()).unwrap(),
    );
    context_values.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    context_values.insert("run_optional_step_x".to_string(), serde_json::json!(false));

    let result = engine.run_full_workflow(context_values).await.unwrap();

    assert!(
        matches!(
            result.status,
            ExecutionStatus::Completed
                | ExecutionStatus::Paused { .. }
                | ExecutionStatus::ElicitationNeeded { .. }
        ),
        "full workflow should return Completed, Paused, or ElicitationNeeded; got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(&storage_dir);
}
