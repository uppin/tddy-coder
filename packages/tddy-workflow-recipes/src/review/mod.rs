//! **Review** workflow: inspect branch changes (elicitation), then structured **`branch-review`** submit → `review.md`.

mod git_context;
mod hooks;
mod parse;
mod persist;
mod prompt;

pub use git_context::{
    format_diff_context_for_prompt, merge_base_commit_for_review,
    merge_base_strategy_documentation, resolve_git_repo_root,
};
pub use hooks::ReviewWorkflowHooks;
pub use parse::{parse_branch_review_output, BranchReviewOutput};
pub use persist::persist_review_md_to_session_dir;
pub use prompt::REVIEW_MD_BASENAME;

/// Graph task id: elicitation / read-only inspect step.
pub(crate) const TASK_INSPECT: &str = "inspect";
/// Graph task id: structured `tddy-tools submit --goal branch-review` step.
pub(crate) const TASK_BRANCH_REVIEW: &str = "branch-review";

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::SessionArtifactManifest;

/// Recipe CLI name and workflow: **`inspect`** (elicitation, no structured submit) → **`branch-review`** → **`end`**.
#[derive(Clone, Copy, Default, Debug)]
pub struct ReviewRecipe;

impl WorkflowRecipe for ReviewRecipe {
    fn name(&self) -> &str {
        "review"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::debug!("review::ReviewRecipe::build_graph marker_id=M001 scope=review::ReviewRecipe::build_graph");
        log::info!("ReviewRecipe::build_graph: inspect -> branch-review -> end");
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let inspect = Arc::new(BackendInvokeTask::from_recipe(
            TASK_INSPECT,
            GoalId::new(TASK_INSPECT),
            recipe.clone(),
            backend.clone(),
        ));
        let branch_review = Arc::new(BackendInvokeTask::from_recipe(
            TASK_BRANCH_REVIEW,
            GoalId::new(TASK_BRANCH_REVIEW),
            recipe,
            backend,
        ));
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("review")
            .add_task(inspect)
            .add_task(branch_review)
            .add_task(end)
            .add_edge(TASK_INSPECT, TASK_BRANCH_REVIEW)
            .add_edge(TASK_BRANCH_REVIEW, "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        log::debug!("ReviewRecipe::create_hooks");
        Arc::new(ReviewWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            TASK_INSPECT | TASK_BRANCH_REVIEW => Some(GoalHints {
                display_name: match goal_id.as_str() {
                    TASK_INSPECT => "Inspect changes".to_string(),
                    _ => "Branch review".to_string(),
                },
                permission: PermissionHint::ReadOnly,
                allowed_tools: crate::permissions::evaluate_allowlist(),
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            _ => None,
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new(TASK_INSPECT), GoalId::new(TASK_BRANCH_REVIEW)]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        if goal_id.as_str() == TASK_BRANCH_REVIEW {
            GoalId::new(TASK_BRANCH_REVIEW)
        } else {
            goal_id.clone()
        }
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Inspect" => Some(GoalId::new(TASK_INSPECT)),
            "BranchReview" => Some(GoalId::new(TASK_BRANCH_REVIEW)),
            "Failed" => None,
            _ => Some(GoalId::new(TASK_INSPECT)),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Inspect")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new(TASK_INSPECT)
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new(TASK_BRANCH_REVIEW)
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool {
        matches!(goal_id.as_str(), TASK_INSPECT | TASK_BRANCH_REVIEW)
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
            "ReviewRecipe::plain_goal_cli_output goal={} session_dir={}",
            goal_id.as_str(),
            session_dir.display()
        );
        if let Some(o) = output {
            log::info!("[review:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        matches!(goal_id.as_str(), TASK_BRANCH_REVIEW)
    }
}

impl SessionArtifactManifest for ReviewRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[("review", REVIEW_MD_BASENAME)]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("review".to_string(), REVIEW_MD_BASENAME.to_string());
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}
