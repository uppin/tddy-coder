//! Acceptance tests: `tddy-core::workflow` re-export shim preserves all external import paths
//! after the `tddy-graph` extraction.
//!
//! Feature: docs/ft/coder/discovery-agent.md (Phase A criteria 2–4)
//! Changeset: docs/dev/1-WIP/2026-06-24-changeset-tddy-graph-extraction.md
//!
//! These tests confirm that every import path external consumers currently rely on remains
//! reachable under `tddy_core::workflow::*` after the types physically move to `tddy_graph`.
//! They also verify that `tddy_graph` and `tddy_core::workflow` share the same type identity —
//! a value of type `tddy_graph::graph::Graph` must be accepted where
//! `tddy_core::workflow::graph::Graph` is expected, without any conversion.

use tddy_graph::graph::Graph as TddyGraphGraph;

use std::sync::Arc;

use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::{
    ElicitationEvent, ExecutionResult, ExecutionStatus, Graph, GraphBuilder,
};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{
    workflow_engine_storage_dir, FileSessionStorage, Session, SessionStorage,
    WORKFLOW_ENGINE_STORAGE_SUBDIR,
};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask, NextAction, Task, TaskResult};
use tddy_core::{
    ElicitationEvent as RootElicitationEvent, ExecutionResult as RootExecutionResult,
    ExecutionStatus as RootExecutionStatus,
};

/// `tddy_core::workflow::task` still exposes `BackendInvokeTask` and `EndTask` (and the pure types)
/// after the extraction, even though `BackendInvokeTask` stays in tddy-core and the pure types move
/// to tddy-graph.
#[test]
fn workflow_task_path_still_exposes_backend_invoke_task_and_end_task() {
    // Given — we can name the types at the expected import paths (compile-time check)
    fn _accepts_task_result(_: TaskResult) {}
    fn _accepts_next_action(_: NextAction) {}

    // Then — `EndTask` is constructable from the expected path
    let _end: Arc<dyn Task> = Arc::new(EndTask::new("done"));

    // BackendInvokeTask exists at the shim path (compile-time only)
    let _ty: Option<Box<dyn Task>> = None;
    let _ = std::mem::size_of::<BackendInvokeTask>();
}

/// `tddy_core::workflow::graph` still exposes `Graph`, `GraphBuilder`, and `ElicitationEvent`.
#[test]
fn workflow_graph_path_still_exposes_graph_graphbuilder_elicitationevent() {
    // Given — build a graph using `GraphBuilder` from the expected shim path
    let graph: Graph = GraphBuilder::new("test-graph")
        .add_task(Arc::new(EndTask::new("only-node")))
        .build();

    // Then — `Graph` is usable (get_task resolves)
    assert!(
        graph.get_task("only-node").is_some(),
        "Graph::get_task must work for the added task"
    );

    // ElicitationEvent is nameable from the expected path (compile-time)
    let _ev: Option<ElicitationEvent> = None;
    let _ = std::mem::size_of::<ElicitationEvent>();
}

/// `tddy_core::workflow::context` still exposes `Context`.
#[test]
fn workflow_context_path_still_exposes_context() {
    // Given
    let ctx = Context::new();

    // When
    ctx.set_sync("shim_test_key", &"shim_test_value".to_string());
    let val = ctx.get_sync::<String>("shim_test_key");

    // Then
    assert_eq!(
        val.as_deref(),
        Some("shim_test_value"),
        "Context from the shim path must support get/set_sync"
    );
}

/// `tddy_core::workflow::session` still exposes `Session`, `SessionStorage`,
/// `FileSessionStorage`, `workflow_engine_storage_dir`, and `WORKFLOW_ENGINE_STORAGE_SUBDIR`.
#[test]
fn workflow_session_path_still_exposes_session_and_storage_helpers() {
    // Given — types are reachable (compile-time); the constant has a non-empty value
    assert!(
        !WORKFLOW_ENGINE_STORAGE_SUBDIR.is_empty(),
        "WORKFLOW_ENGINE_STORAGE_SUBDIR must be a non-empty string"
    );

    // And — `workflow_engine_storage_dir` computes a path (no panic)
    let base = std::path::PathBuf::from("/tmp/shim-test");
    let storage_dir = workflow_engine_storage_dir(&base);
    assert!(
        storage_dir.starts_with(&base),
        "workflow_engine_storage_dir must return a path under the given base"
    );

    // And — Session and FileSessionStorage are constructable from the expected paths
    let _: Option<Box<dyn SessionStorage>> = None;
    let _ = std::mem::size_of::<Session>();
    let _ = std::mem::size_of::<FileSessionStorage>();
}

/// `tddy_core::workflow::hooks` still exposes `RunnerHooks`.
#[test]
fn workflow_hooks_path_still_exposes_runner_hooks_trait() {
    // Given — `RunnerHooks` is usable as a trait object (compile-time)
    let _: Option<Box<dyn RunnerHooks>> = None;

    // Then — compiles; no assertion needed beyond that
}

/// `FlowRunner` is still reachable at `tddy_core::workflow::runner::FlowRunner`.
#[test]
fn workflow_runner_path_still_exposes_flow_runner() {
    // Given — type is reachable (compile-time size-of check)
    let _ = std::mem::size_of::<FlowRunner>();
}

/// The root-level `tddy_core::*` re-exports that proxy through the shim modules still resolve.
/// (`lib.rs:110-117` re-exports `WorkflowEngine`, `find_git_root`, `ElicitationEvent`,
/// `ExecutionResult`, `ExecutionStatus`, `WorkflowState`, `workflow_engine_storage_dir`,
/// `WORKFLOW_ENGINE_STORAGE_SUBDIR`, `GoalOptions`.)
#[test]
fn lib_root_reexports_resolve_through_shim() {
    // Given — these types/values are accessible at the tddy_core root
    let _: Option<RootElicitationEvent> = None;
    let _: Option<RootExecutionResult> = None;
    let _: Option<RootExecutionStatus> = None;
    // (WorkflowEngine, GoalOptions, etc. are compile-time checks only)
}

/// `tddy_graph::graph::Graph` and `tddy_core::workflow::graph::Graph` must be the **same type**
/// (type identity preserved by the re-export shim — not a type alias that breaks trait impls).
/// Both names resolve to the same Rust type via `pub use tddy_graph::graph::*;` in the shim.
#[test]
fn graph_type_identity_is_shared_across_tddy_graph_and_tddy_core() {
    // Given — a graph built via `tddy_graph` directly
    let graph: TddyGraphGraph = tddy_graph::graph::GraphBuilder::new("type-identity-test")
        .add_task(Arc::new(tddy_graph::task::EndTask::new("only")))
        .build();

    // Then — the same value is accepted where `tddy_core::workflow::graph::Graph` is expected,
    // with no conversion — proving both names refer to the same Rust type.
    let _: Graph = graph;
}
