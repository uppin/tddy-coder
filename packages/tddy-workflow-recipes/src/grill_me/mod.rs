//! **Grill-me** workflow: **Grill** (questions via `InvokeResponse.questions`), then **Create plan** (`grill-me-brief.md`).

mod hooks;
mod prompt;
pub mod repo_plan;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub use hooks::GrillMeWorkflowHooks;
pub use prompt::{create_plan_system_prompt, grill_system_prompt, GRILL_ME_BRIEF_BASENAME};
pub use repo_plan::{persisted_grill_me_brief_path, GrillMePersistedBriefPathError};

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::SessionArtifactManifest;

/// **Grill-me** recipe: **`grill`** → **`create-plan`** → **`end`**.
#[derive(Clone, Copy, Default, Debug)]
pub struct GrillMeRecipe;

impl WorkflowRecipe for GrillMeRecipe {
    fn name(&self) -> &str {
        "grill-me"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::info!("GrillMeRecipe::build_graph: grill -> create-plan -> end");
        let grill = Arc::new(BackendInvokeTask::from_recipe(
            "grill",
            GoalId::new("grill"),
            self,
            backend.clone(),
        ));
        let create_plan = Arc::new(BackendInvokeTask::from_recipe(
            "create-plan",
            GoalId::new("create-plan"),
            self,
            backend,
        ));
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("grill_me")
            .add_task(grill)
            .add_task(create_plan)
            .add_task(end)
            .add_edge("grill", "create-plan")
            .add_edge("create-plan", "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        log::debug!("GrillMeRecipe::create_hooks");
        Arc::new(GrillMeWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            "grill" => Some(GoalHints {
                display_name: "Grill".to_string(),
                permission: PermissionHint::AcceptEdits,
                allowed_tools: vec![],
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            "create-plan" => Some(GoalHints {
                display_name: "Create plan".to_string(),
                permission: PermissionHint::AcceptEdits,
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
        vec![GoalId::new("grill"), GoalId::new("create-plan")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Grill" => Some(GoalId::new("grill")),
            "CreatePlan" => Some(GoalId::new("create-plan")),
            "Failed" => None,
            _ => Some(GoalId::new("grill")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Grill")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("grill")
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new("create-plan")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool {
        matches!(goal_id.as_str(), "grill" | "create-plan")
    }

    fn uses_primary_session_document(&self) -> bool {
        false
    }

    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &Path,
    ) -> Result<(), String> {
        log::info!(
            "GrillMeRecipe::plain_goal_cli_output goal={} session_dir={}",
            goal_id.as_str(),
            session_dir.display()
        );
        if let Some(o) = output {
            log::info!("[grill-me:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        !matches!(goal_id.as_str(), "grill" | "create-plan")
    }
}

#[cfg(test)]
mod plan_refinement_tests {
    use super::GrillMeRecipe;
    use tddy_core::GoalId;
    use tddy_core::WorkflowRecipe;

    #[test]
    fn plan_refinement_goal_is_create_plan_not_grill() {
        let r = GrillMeRecipe;
        assert_eq!(r.plan_refinement_goal(), GoalId::new("create-plan"));
        assert_ne!(r.plan_refinement_goal(), r.start_goal());
    }
}

impl SessionArtifactManifest for GrillMeRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[("grill_brief", GRILL_ME_BRIEF_BASENAME)]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert(
            "grill_brief".to_string(),
            GRILL_ME_BRIEF_BASENAME.to_string(),
        );
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}
