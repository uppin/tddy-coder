//! Bug-fix workflow: **analyze** → **reproduce** with clarification questions (demo-ready recipe).

mod analyze;
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

/// Bug-fix recipe: **`analyze`** then **`reproduce`** with agent output and clarification questions.
#[derive(Clone, Copy, Default, Debug)]
pub struct BugfixRecipe;

impl WorkflowRecipe for BugfixRecipe {
    fn name(&self) -> &str {
        "bugfix"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::debug!("[bugfix recipe] build_graph: analyze → reproduce → end");
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let analyze = Arc::new(BackendInvokeTask::from_recipe(
            "analyze",
            GoalId::new("analyze"),
            recipe.clone(),
            backend.clone(),
        ));
        let reproduce = Arc::new(BackendInvokeTask::from_recipe(
            "reproduce",
            GoalId::new("reproduce"),
            recipe,
            backend,
        ));
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("bugfix")
            .add_task(analyze)
            .add_task(reproduce)
            .add_task(end)
            .add_edge("analyze", "reproduce")
            .add_edge("reproduce", "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(BugfixWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            "analyze" => {
                log::debug!("[bugfix recipe] goal_hints(analyze)");
                Some(GoalHints {
                    display_name: "Analyze".to_string(),
                    permission: PermissionHint::ReadOnly,
                    allowed_tools: vec![],
                    default_model: None,
                    agent_output: true,
                    agent_cli_plan_mode: false,
                    claude_nonzero_exit_ok_if_structured_response: false,
                })
            }
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
        vec![GoalId::new("analyze"), GoalId::new("reproduce")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    /// Resume goal for persisted [`WorkflowState`].
    ///
    /// **`Reproducing`** means analyze completed successfully (changeset updated to that state);
    /// the next runnable goal is **`reproduce`**. Any other non-**`Failed`** state—including
    /// **`Init`**, **`Analyzing`**, or legacy/unknown strings—maps to **`analyze`** so partial or
    /// upgraded sessions still land on the first step unless we are clearly in the reproduce phase.
    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Failed" => None,
            "Reproducing" => Some(GoalId::new("reproduce")),
            _ => Some(GoalId::new("analyze")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            "Analyzing" => "Analyzing",
            "Reproducing" => "Reproducing",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Init")
    }

    fn start_goal(&self) -> GoalId {
        log::debug!("[bugfix recipe] start_goal = analyze");
        GoalId::new("analyze")
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
        matches!(goal_id.as_str(), "analyze")
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
    use std::sync::Arc;
    use tddy_core::backend::StubBackend;
    use tddy_core::workflow::context::Context;
    use tddy_core::workflow::ids::WorkflowState;
    use tddy_core::WorkflowRecipe;

    /// Acceptance: graph must be `analyze` → `reproduce` → `end` (task ids match goal ids).
    #[test]
    fn bugfix_graph_orders_analyze_before_reproduce() {
        let backend = Arc::new(StubBackend::new());
        let recipe = BugfixRecipe;
        let graph = recipe.build_graph(backend);
        let ctx = Context::new();
        assert_eq!(
            graph.next_task_id("analyze", &ctx),
            Some("reproduce".to_string()),
            "edge analyze → reproduce"
        );
        assert_eq!(
            graph.next_task_id("reproduce", &ctx),
            Some("end".to_string()),
            "edge reproduce → end"
        );
        assert_eq!(
            recipe.status_for_state(&WorkflowState::new("Analyzing")),
            "Analyzing",
            "PRD: presenter distinguishes analyzing vs reproducing"
        );
    }

    #[test]
    fn bugfix_recipe_is_valid_plugin() {
        let r: std::sync::Arc<dyn WorkflowRecipe> = std::sync::Arc::new(BugfixRecipe);
        assert_eq!(r.name(), "bugfix");
        assert_eq!(r.start_goal().as_str(), "analyze");
        let goal_ids = r.goal_ids();
        let ids: Vec<&str> = goal_ids.iter().map(|g| g.as_str()).collect();
        assert!(
            ids.contains(&"analyze") && ids.contains(&"reproduce"),
            "goal_ids must include analyze and reproduce: {:?}",
            ids
        );
        assert_eq!(
            r.status_for_state(&WorkflowState::new("Reproducing")),
            "Reproducing",
            "PRD: status_for_state must label reproduce phase"
        );
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
