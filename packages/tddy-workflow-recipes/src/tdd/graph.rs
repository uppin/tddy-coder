//! TDD workflow graph builder.
//!
//! Constructs the graph of step Tasks with edges and conditional edges.
//! Uses PlanTask for plan (writes PRD.md with TODO section); BackendInvokeTask for
//! acceptance-tests, red, green, demo, evaluate, validate, refactor.

use crate::tdd::plan_task::PlanTask;
use std::sync::Arc;
use tddy_core::backend::{CodingBackend, GoalId, WorkflowRecipe};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

/// Bumped to **2** when interview→plan handoff relay and `before_plan` merge are fully implemented.
pub const TDD_INTERVIEW_GRAPH_HANDOFF_VERSION: u32 = 2;

/// Build the TDD workflow graph (interview -> plan -> acceptance-tests -> red -> green -> end).
///
/// Uses [`BackendInvokeTask`] for **interview**; [`PlanTask`] for **plan** (writes PRD.md with TODO
/// section); [`BackendInvokeTask`] for acceptance-tests, red, green. For tddy-demo with
/// StubBackend, produces a working graph with plan artifacts.
pub fn build_tdd_workflow_graph(
    backend: Arc<dyn CodingBackend>,
    recipe: Arc<dyn WorkflowRecipe>,
) -> Graph {
    let interview = Arc::new(BackendInvokeTask::from_recipe(
        "interview",
        GoalId::new("interview"),
        recipe.clone(),
        backend.clone(),
    ));
    let plan = Arc::new(PlanTask::new(backend.clone(), recipe.clone()));
    let acc = Arc::new(BackendInvokeTask::from_recipe(
        "acceptance-tests",
        GoalId::new("acceptance-tests"),
        recipe.clone(),
        backend.clone(),
    ));
    let red = Arc::new(BackendInvokeTask::from_recipe(
        "red",
        GoalId::new("red"),
        recipe.clone(),
        backend.clone(),
    ));
    let green = Arc::new(BackendInvokeTask::from_recipe(
        "green",
        GoalId::new("green"),
        recipe.clone(),
        backend.clone(),
    ));
    let end = Arc::new(EndTask::new("end"));

    GraphBuilder::new("tdd_workflow")
        .add_task(interview)
        .add_task(plan)
        .add_task(acc.clone())
        .add_task(red.clone())
        .add_task(green.clone())
        .add_task(end)
        .add_edge("interview", "plan")
        .add_edge("plan", "acceptance-tests")
        .add_edge("acceptance-tests", "red")
        .add_edge("red", "green")
        .add_edge("green", "end")
        .build()
}

/// Build the full TDD workflow graph including demo, evaluate, validate, refactor.
///
/// Topology: plan -> acceptance-tests -> red -> green -> (conditional: demo | evaluate) -> validate -> refactor -> end.
/// When context has `run_optional_step_x`: true, green transitions to demo; else to evaluate.
/// Demo always transitions to evaluate.
pub fn build_full_tdd_workflow_graph(
    backend: Arc<dyn CodingBackend>,
    recipe: Arc<dyn WorkflowRecipe>,
) -> Graph {
    let interview = Arc::new(BackendInvokeTask::from_recipe(
        "interview",
        GoalId::new("interview"),
        recipe.clone(),
        backend.clone(),
    ));
    let plan = Arc::new(PlanTask::new(backend.clone(), recipe.clone()));
    let acc = Arc::new(BackendInvokeTask::from_recipe(
        "acceptance-tests",
        GoalId::new("acceptance-tests"),
        recipe.clone(),
        backend.clone(),
    ));
    let red = Arc::new(BackendInvokeTask::from_recipe(
        "red",
        GoalId::new("red"),
        recipe.clone(),
        backend.clone(),
    ));
    let green = Arc::new(BackendInvokeTask::from_recipe(
        "green",
        GoalId::new("green"),
        recipe.clone(),
        backend.clone(),
    ));
    let demo = Arc::new(BackendInvokeTask::from_recipe(
        "demo",
        GoalId::new("demo"),
        recipe.clone(),
        backend.clone(),
    ));
    let evaluate = Arc::new(BackendInvokeTask::from_recipe(
        "evaluate",
        GoalId::new("evaluate"),
        recipe.clone(),
        backend.clone(),
    ));
    let validate = Arc::new(BackendInvokeTask::from_recipe(
        "validate",
        GoalId::new("validate"),
        recipe.clone(),
        backend.clone(),
    ));
    let refactor = Arc::new(BackendInvokeTask::from_recipe(
        "refactor",
        GoalId::new("refactor"),
        recipe.clone(),
        backend.clone(),
    ));
    let update_docs = Arc::new(BackendInvokeTask::from_recipe(
        "update-docs",
        GoalId::new("update-docs"),
        recipe.clone(),
        backend.clone(),
    ));
    let end = Arc::new(EndTask::new("end"));

    GraphBuilder::new("tdd_full_workflow")
        .add_task(interview)
        .add_task(plan)
        .add_task(acc.clone())
        .add_task(red.clone())
        .add_task(green.clone())
        .add_task(demo.clone())
        .add_task(evaluate.clone())
        .add_task(validate.clone())
        .add_task(refactor.clone())
        .add_task(update_docs.clone())
        .add_task(end)
        .add_edge("interview", "plan")
        .add_edge("plan", "acceptance-tests")
        .add_edge("acceptance-tests", "red")
        .add_edge("red", "green")
        .add_conditional_edge(
            "green",
            |ctx| ctx.get_sync::<bool>("run_optional_step_x").unwrap_or(false),
            "demo",
            "evaluate",
        )
        .add_edge("demo", "evaluate")
        .add_edge("evaluate", "validate")
        .add_edge("validate", "refactor")
        .add_edge("refactor", "update-docs")
        .add_edge("update-docs", "end")
        .build()
}
