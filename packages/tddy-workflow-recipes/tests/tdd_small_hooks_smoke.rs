//! Smoke test: `TddSmallWorkflowHooks` wires [`RunnerHooks`] without panicking on trivial calls.

use std::sync::Arc;

use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_workflow_recipes::{SessionArtifactManifest, TddSmallRecipe, TddSmallWorkflowHooks};

#[test]
fn tdd_small_hooks_before_task_unknown_task_is_noop() {
    let recipe: Arc<dyn tddy_core::WorkflowRecipe> = Arc::new(TddSmallRecipe);
    let manifest: Arc<dyn SessionArtifactManifest> = Arc::new(TddSmallRecipe);
    let hooks = TddSmallWorkflowHooks::new(recipe, manifest);
    let ctx = Context::new();
    hooks
        .before_task("nonexistent-task", &ctx)
        .expect("unknown task should be ignored");
}
