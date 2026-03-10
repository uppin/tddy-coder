//! TDD workflow graph builder.
//!
//! Constructs the graph of step Tasks with edges and conditional edges.
//! Uses PlanTask for plan (writes PRD.md); BackendInvokeTask for
//! acceptance-tests, red, green, demo, evaluate, validate, refactor.

use crate::backend::{CodingBackend, Goal};
use crate::workflow::graph::{Graph, GraphBuilder};
use crate::workflow::steps::PlanTask;
use crate::workflow::task::{BackendInvokeTask, EndTask};
use std::sync::Arc;

/// Build the TDD workflow graph (plan -> acceptance-tests -> red -> green -> end).
///
/// Uses PlanTask for plan (writes PRD.md); BackendInvokeTask for
/// acceptance-tests, red, green. For tddy-demo with StubBackend, produces
/// a working graph with plan artifacts.
pub fn build_tdd_workflow_graph(backend: Arc<dyn CodingBackend>) -> Graph {
    let plan = Arc::new(PlanTask::new(backend.clone()));
    let acc = Arc::new(BackendInvokeTask::new(
        "acceptance-tests",
        Goal::AcceptanceTests,
        backend.clone(),
    ));
    let red = Arc::new(BackendInvokeTask::new("red", Goal::Red, backend.clone()));
    let green = Arc::new(BackendInvokeTask::new(
        "green",
        Goal::Green,
        backend.clone(),
    ));
    let end = Arc::new(EndTask::new("end"));

    GraphBuilder::new("tdd_workflow")
        .add_task(plan)
        .add_task(acc.clone())
        .add_task(red.clone())
        .add_task(green.clone())
        .add_task(end)
        .add_edge("plan", "acceptance-tests")
        .add_edge("acceptance-tests", "red")
        .add_edge("red", "green")
        .add_edge("green", "end")
        .build()
}

/// Build the full TDD workflow graph including demo, evaluate, validate, refactor.
///
/// Topology: plan -> acceptance-tests -> red -> green -> (conditional: demo | evaluate) -> validate -> refactor -> end.
/// When context has "run_demo": true, green transitions to demo; else to evaluate.
/// Demo always transitions to evaluate.
pub fn build_full_tdd_workflow_graph(backend: Arc<dyn CodingBackend>) -> Graph {
    let plan = Arc::new(PlanTask::new(backend.clone()));
    let acc = Arc::new(BackendInvokeTask::new(
        "acceptance-tests",
        Goal::AcceptanceTests,
        backend.clone(),
    ));
    let red = Arc::new(BackendInvokeTask::new("red", Goal::Red, backend.clone()));
    let green = Arc::new(BackendInvokeTask::new(
        "green",
        Goal::Green,
        backend.clone(),
    ));
    let demo = Arc::new(BackendInvokeTask::new("demo", Goal::Demo, backend.clone()));
    let evaluate = Arc::new(BackendInvokeTask::new(
        "evaluate",
        Goal::Evaluate,
        backend.clone(),
    ));
    let validate = Arc::new(BackendInvokeTask::new(
        "validate",
        Goal::Validate,
        backend.clone(),
    ));
    let refactor = Arc::new(BackendInvokeTask::new(
        "refactor",
        Goal::Refactor,
        backend.clone(),
    ));
    let end = Arc::new(EndTask::new("end"));

    GraphBuilder::new("tdd_full_workflow")
        .add_task(plan)
        .add_task(acc.clone())
        .add_task(red.clone())
        .add_task(green.clone())
        .add_task(demo.clone())
        .add_task(evaluate.clone())
        .add_task(validate.clone())
        .add_task(refactor.clone())
        .add_task(end)
        .add_edge("plan", "acceptance-tests")
        .add_edge("acceptance-tests", "red")
        .add_edge("red", "green")
        .add_conditional_edge(
            "green",
            |ctx| ctx.get_sync::<bool>("run_demo").unwrap_or(false),
            "demo",
            "evaluate",
        )
        .add_edge("demo", "evaluate")
        .add_edge("evaluate", "validate")
        .add_edge("validate", "refactor")
        .add_edge("refactor", "end")
        .build()
}
