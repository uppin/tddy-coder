//! Graph builder for the `tdd-small` workflow recipe.
//!
//! Topology: `plan` → `red` → `green` → `post-green-review` → `refactor` → `update-docs` → `end`.
//! No standalone `acceptance-tests`, `demo`, or separate `evaluate` / `validate` tasks.

use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalId};
use tddy_core::workflow::graph::Graph;
use tddy_core::workflow::graph::GraphBuilder;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::tdd::plan_task::PlanTask;

/// Build the `tdd-small` workflow graph.
pub fn build_tdd_small_workflow_graph(
    backend: Arc<dyn CodingBackend>,
    recipe: Arc<dyn WorkflowRecipe>,
) -> Graph {
    log::info!(
        "build_tdd_small_workflow_graph: wiring plan→red→green→post-green-review→refactor→update-docs→end"
    );
    let plan = Arc::new(PlanTask::new(backend.clone(), recipe.clone()));
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
    let post_green = Arc::new(BackendInvokeTask::from_recipe(
        "post-green-review",
        GoalId::new("post-green-review"),
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

    GraphBuilder::new("tdd_small_workflow")
        .add_task(plan)
        .add_task(red.clone())
        .add_task(green.clone())
        .add_task(post_green.clone())
        .add_task(refactor.clone())
        .add_task(update_docs.clone())
        .add_task(end)
        .add_edge("plan", "red")
        .add_edge("red", "green")
        .add_edge("green", "post-green-review")
        .add_edge("post-green-review", "refactor")
        .add_edge("refactor", "update-docs")
        .add_edge("update-docs", "end")
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tddy_core::backend::StubBackend;
    use tddy_core::workflow::recipe::WorkflowRecipe;

    use crate::tdd_small::TddSmallRecipe;

    #[test]
    fn tdd_small_graph_task_count_matches_full_topology() {
        let backend = Arc::new(StubBackend::new());
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddSmallRecipe);
        let g = build_tdd_small_workflow_graph(backend, recipe);
        assert_eq!(
            g.task_ids().count(),
            7,
            "tdd-small graph must have seven tasks (plan through end)"
        );
    }
}
