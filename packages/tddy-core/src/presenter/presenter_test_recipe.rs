//! Minimal [`WorkflowRecipe`] for presenter unit tests only.
//! `tddy-core` must not dev-depend on `tddy-workflow-recipes` (avoids two copies of this crate
//! and broken `dyn WorkflowRecipe` casts).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::backend::CodingBackend;
use crate::workflow::graph::GraphBuilder;
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::ids::{GoalId, WorkflowState};
use crate::workflow::recipe::{GoalHints, PermissionHint, WorkflowEventSender, WorkflowRecipe};

/// Mirrors `tddy_workflow_recipes::permissions::plan_allowlist` for this test-only recipe.
/// `tddy-core` does not depend on recipes; keep entries aligned when plan tools change.
fn presenter_test_plan_allowed_tools() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Glob".to_string(),
        "Grep".to_string(),
        "SemanticSearch".to_string(),
        "AskUserQuestion".to_string(),
        "ExitPlanMode".to_string(),
        "Bash(tddy-tools *)".to_string(),
    ]
}

/// No-op hooks used when presenter tests never start a real workflow.
struct NoopHooks;

impl RunnerHooks for NoopHooks {
    fn before_task(
        &self,
        _task_id: &str,
        _context: &crate::workflow::context::Context,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    fn after_task(
        &self,
        _task_id: &str,
        _context: &crate::workflow::context::Context,
        _result: &crate::workflow::task::TaskResult,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    fn on_error(
        &self,
        _task_id: &str,
        _context: &crate::workflow::context::Context,
        _error: &(dyn std::error::Error + Send + Sync),
    ) {
    }
}

/// Empty graph + noop hooks — sufficient for presenter tests that only inject events.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyPresenterTestRecipe;

impl WorkflowRecipe for EmptyPresenterTestRecipe {
    fn name(&self) -> &str {
        "empty_presenter_test"
    }

    fn build_graph(&self, _backend: Arc<dyn CodingBackend>) -> crate::workflow::graph::Graph {
        GraphBuilder::new("empty_presenter_test").build()
    }

    fn create_hooks(&self, _event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(NoopHooks)
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        if goal_id.as_str() == "plan" {
            Some(GoalHints {
                display_name: "plan".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: presenter_test_plan_allowed_tools(),
                default_model: None,
                agent_output: false,
                planning_mode_intent: true,
                claude_nonzero_exit_ok_if_structured_response: true,
            })
        } else {
            None
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new("plan")]
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
        WorkflowState::new("Init")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("plan")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("prd".to_string(), "PRD.md".to_string());
        m
    }

    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[("prd", "PRD.md")]
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
