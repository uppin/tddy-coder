//! TDD workflow graph builder.
//!
//! Constructs the graph of step Tasks with edges and conditional edges.
//! Uses PlanTask for plan (writes PRD.md, TODO.md); BackendInvokeTask for
//! acceptance-tests, red, green.

use crate::backend::{CodingBackend, Goal};
use crate::workflow::graph::{Graph, GraphBuilder};
use crate::workflow::steps::PlanTask;
use crate::workflow::task::{BackendInvokeTask, EndTask};
use std::sync::Arc;

/// Build the TDD workflow graph.
///
/// Uses PlanTask for plan (writes PRD.md, TODO.md); BackendInvokeTask for
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
