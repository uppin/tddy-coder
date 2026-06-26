//! **orchestrate-pr-stack** recipe: resumable idempotent loop that merges a PR stack to master.

mod actions;
mod assess;
pub mod bridge;
mod git_ops;
pub mod github;
mod hooks;
pub mod transient;

pub use actions::{MergeTask, RepointTask, SpawnTask};
pub use bridge::{execute_stack_merge, execute_stack_repoint, seed_orchestrator_stack_from_plan};
pub use assess::{
    assemble_views, decide_next_action, effective_base_ref, AssessTask, ChildPhase, NodeView,
    OrchestratorAction, PrLiveStatus,
};
pub use github::{GithubPrApi, RealGithubPrApi};
pub use hooks::OrchestratePrStackHooks;
pub use transient::{
    recover_in_flight_stack_op, write_stack_op_journal, MergePhase, StackOpJournal,
};

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::EndTask;

use crate::SessionArtifactManifest;

pub const STACK_STATUS_MD_BASENAME: &str = "stack-status.md";
pub const STACK_STATUS_JSON_BASENAME: &str = "stack-status.json";

/// **orchestrate-pr-stack** recipe: assess → spawn/merge/repoint → loop back → end.
#[derive(Clone, Copy, Default, Debug)]
pub struct OrchestratePrStackRecipe;

impl WorkflowRecipe for OrchestratePrStackRecipe {
    fn name(&self) -> &str {
        "orchestrate-pr-stack"
    }

    fn build_graph(&self, _backend: Arc<dyn CodingBackend>) -> Graph {
        let assess = Arc::new(AssessTask::new());
        let spawn = Arc::new(SpawnTask::new());
        let merge = Arc::new(MergeTask::new());
        let repoint = Arc::new(RepointTask::new());
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("orchestrate_pr_stack")
            .add_task(assess)
            .add_task(spawn)
            .add_task(merge)
            .add_task(repoint)
            .add_task(end)
            .add_edge("assess", "spawn")
            .add_edge("assess", "merge")
            .add_edge("assess", "repoint")
            .add_edge("assess", "end")
            .add_edge("spawn", "assess")
            .add_edge("merge", "repoint")
            .add_edge("repoint", "assess")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(OrchestratePrStackHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            "assess" => Some(GoalHints {
                display_name: "Assess stack".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: false,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            _ => None,
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new("assess")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Done" | "Failed" => None,
            _ => Some(GoalId::new("assess")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Done" => "Completed",
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Init")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("assess")
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new("assess")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool {
        true
    }

    fn uses_primary_session_document(&self) -> bool {
        false
    }

    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        _session_dir: &Path,
    ) -> Result<(), String> {
        if let Some(o) = output {
            log::info!("[orchestrate-pr-stack:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, _goal_id: &GoalId) -> bool {
        false
    }
}

impl SessionArtifactManifest for OrchestratePrStackRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("stack_status_md", STACK_STATUS_MD_BASENAME),
            ("stack_status_json", STACK_STATUS_JSON_BASENAME),
        ]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert(
            "stack_status_md".to_string(),
            STACK_STATUS_MD_BASENAME.to_string(),
        );
        a.insert(
            "stack_status_json".to_string(),
            STACK_STATUS_JSON_BASENAME.to_string(),
        );
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}
