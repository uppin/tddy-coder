//! Test-only helpers shared across `tddy-core` unit tests.
//!
//! `tddy-core` must not depend on `tddy-workflow-recipes` (that crate depends on this one), so
//! minimal in-crate recipes live here.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::backend::{CodingBackend, GoalHints, PermissionHint};
use crate::workflow::graph::{Graph, GraphBuilder};
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::ids::{GoalId, WorkflowState};
use crate::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_graph::task::EndTask;

/// A minimal linear recipe: `a -> b -> c -> end`. `goal_instructions` returns distinctive text.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearTestRecipe;

impl WorkflowRecipe for LinearTestRecipe {
    fn name(&self) -> &str {
        "linear_test"
    }

    fn build_graph(&self, _backend: Arc<dyn CodingBackend>) -> Graph {
        GraphBuilder::new("linear_test")
            .add_task(Arc::new(EndTask::new("a")))
            .add_task(Arc::new(EndTask::new("b")))
            .add_task(Arc::new(EndTask::new("c")))
            .add_task(Arc::new(EndTask::new("end")))
            .add_edge("a", "b")
            .add_edge("b", "c")
            .add_edge("c", "end")
            .build()
    }

    fn create_hooks(&self, _tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        unimplemented!("LinearTestRecipe is not driven by the engine")
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        Some(GoalHints {
            display_name: goal_id.to_string(),
            permission: PermissionHint::AcceptEdits,
            allowed_tools: vec![format!("Tool_{goal_id}")],
            default_model: None,
            agent_output: true,
            agent_cli_plan_mode: false,
            claude_nonzero_exit_ok_if_structured_response: false,
        })
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        ["a", "b", "c"].into_iter().map(GoalId::new).collect()
    }

    fn goal_instructions(&self, goal_id: &GoalId) -> String {
        format!("INSTRUCTIONS:{goal_id}")
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, _state: &WorkflowState) -> Option<GoalId> {
        None
    }

    fn status_for_state(&self, _state: &WorkflowState) -> &'static str {
        "Active"
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("a")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("a")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool {
        false
    }

    fn plain_goal_cli_output(
        &self,
        _goal_id: &GoalId,
        _output: Option<&str>,
        _session_dir: &Path,
    ) -> Result<(), String> {
        Ok(())
    }
}
