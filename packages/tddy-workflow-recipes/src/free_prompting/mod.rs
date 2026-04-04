//! Free-prompting workflow recipe (F1): single **Prompting** loop without the TDD pipeline.

mod hooks;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub use hooks::FreePromptingWorkflowHooks;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::BackendInvokeTask;

use crate::SessionArtifactManifest;

/// Single **Prompting** task: invokes the coding backend for `prompting`, then pauses (no `end` edge)
/// so multi-turn chat stays in [`AppMode::Running`] until the user ends the session.
#[derive(Clone, Copy, Default, Debug)]
pub struct FreePromptingRecipe;

impl WorkflowRecipe for FreePromptingRecipe {
    fn name(&self) -> &str {
        log::debug!("FreePromptingRecipe::name -> free-prompting");
        "free-prompting"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::info!(
            "FreePromptingRecipe::build_graph: single prompting (backend invoke, no end edge)"
        );
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let prompting = Arc::new(BackendInvokeTask::from_recipe(
            "prompting",
            GoalId::new("prompting"),
            recipe,
            backend,
        ));

        GraphBuilder::new("free_prompting")
            .add_task(prompting)
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        log::debug!("FreePromptingRecipe::create_hooks");
        Arc::new(FreePromptingWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        if goal_id.as_str() != "prompting" {
            return None;
        }
        Some(GoalHints {
            display_name: "Prompting".to_string(),
            permission: PermissionHint::AcceptEdits,
            allowed_tools: vec![],
            default_model: None,
            agent_output: true,
            agent_cli_plan_mode: false,
            claude_nonzero_exit_ok_if_structured_response: false,
        })
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new("prompting")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Prompting" => Some(GoalId::new("prompting")),
            "Failed" => None,
            _ => Some(GoalId::new("prompting")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        log::debug!("FreePromptingRecipe::initial_state -> Prompting");
        WorkflowState::new("Prompting")
    }

    fn start_goal(&self) -> GoalId {
        log::info!("FreePromptingRecipe::start_goal -> prompting");
        GoalId::new("prompting")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool {
        false
    }

    fn uses_primary_session_document(&self) -> bool {
        log::debug!("FreePromptingRecipe::uses_primary_session_document -> false");
        false
    }

    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &Path,
    ) -> Result<(), String> {
        log::info!(
            "FreePromptingRecipe::plain_goal_cli_output goal={} session_dir={}",
            goal_id.as_str(),
            session_dir.display()
        );
        if let Some(o) = output {
            log::info!("[free-prompting] output:\n{}", o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        goal_id.as_str() != "prompting"
    }
}

impl SessionArtifactManifest for FreePromptingRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}
