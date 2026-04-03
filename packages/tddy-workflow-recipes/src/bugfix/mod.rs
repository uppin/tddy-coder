//! Bug-fix workflow: **Reproduce** with clarification questions (demo-ready recipe).

mod hooks;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub use hooks::BugfixWorkflowHooks;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::SessionArtifactManifest;

/// Bug-fix recipe: invokes the backend for `reproduce` with agent output and clarification questions.
#[derive(Clone, Copy, Default, Debug)]
pub struct BugfixRecipe;

impl WorkflowRecipe for BugfixRecipe {
    fn name(&self) -> &str {
        "bugfix"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        let reproduce = Arc::new(BackendInvokeTask::from_recipe(
            "reproduce",
            GoalId::new("reproduce"),
            self,
            backend,
        ));
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("bugfix")
            .add_task(reproduce)
            .add_task(end)
            .add_edge("reproduce", "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(BugfixWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            "reproduce" => Some(GoalHints {
                display_name: "Reproduce".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            _ => None,
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new("reproduce")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Reproducing" => Some(GoalId::new("reproduce")),
            "Failed" => None,
            _ => Some(GoalId::new("reproduce")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Init")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("reproduce")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool {
        false
    }

    fn uses_primary_session_document(&self) -> bool {
        false
    }

    fn read_primary_session_document_utf8(&self, session_dir: &Path) -> Option<String> {
        self.primary_document_basename()
            .and_then(|b| tddy_workflow::read_session_artifact_utf8(session_dir, &b))
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        goal_id.as_str() != "reproduce"
    }

    fn plain_goal_cli_output(
        &self,
        _goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &Path,
    ) -> Result<(), String> {
        if let Some(o) = output {
            println!("{}", o);
        }
        println!("\nSession dir: {}", session_dir.display());
        Ok(())
    }
}

impl SessionArtifactManifest for BugfixRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[("fix_plan", "fix-plan.md")]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("fix_plan".to_string(), "fix-plan.md".to_string());
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        Some("fix-plan.md".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::WorkflowRecipe;

    #[test]
    fn bugfix_recipe_is_valid_plugin() {
        let r: std::sync::Arc<dyn WorkflowRecipe> = std::sync::Arc::new(BugfixRecipe);
        assert_eq!(r.name(), "bugfix");
        assert_eq!(r.start_goal().as_str(), "reproduce");
        assert_eq!(r.goal_ids().len(), 1);
    }

    #[test]
    fn bugfix_reproduce_does_not_require_tddy_tools_submit() {
        let r = BugfixRecipe;
        assert!(
            !r.goal_requires_tddy_tools_submit(&GoalId::new("reproduce")),
            "reproduce goal should not require tddy-tools submit in demo"
        );
    }
}
