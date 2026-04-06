//! **merge-pr** workflow: analyze merge feasibility, sync with default branch, resolve conflicts
//! with the agent, finalize with structured `merge-pr-report` submit + optional GitHub merge + push.

mod hooks;

pub mod git_ops;
pub mod github;

pub use hooks::MergePrWorkflowHooks;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::permissions;
use crate::SessionArtifactManifest;

/// Graph task: read-only analysis — fetch origin, diff against main, report conflict status.
pub const TASK_ANALYZE: &str = "analyze";
/// Graph task: integrate `origin/main` into the feature branch (agent-assisted conflict resolution).
pub const TASK_SYNC_MAIN: &str = "sync-main";
/// Graph task: structured `merge-pr-report` submit + push + optional GitHub REST merge.
pub const TASK_FINALIZE: &str = "finalize";

const MERGE_PR_ANALYZE_GOAL: &str = "merge-pr-analyze";
const MERGE_PR_REPORT_GOAL: &str = "merge-pr-report";
const MERGE_PR_REPORT_BASENAME: &str = "merge-pr-report.json";

/// Shipped **merge-pr** workflow recipe (CLI name `merge-pr`).
#[derive(Clone, Copy, Default, Debug)]
pub struct MergePrRecipe;

impl WorkflowRecipe for MergePrRecipe {
    fn name(&self) -> &str {
        "merge-pr"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::info!("MergePrRecipe::build_graph: analyze -> sync-main -> finalize -> end");
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let analyze = Arc::new(BackendInvokeTask::from_recipe(
            TASK_ANALYZE,
            GoalId::new(TASK_ANALYZE),
            recipe.clone(),
            backend.clone(),
        ));
        let sync = Arc::new(BackendInvokeTask::from_recipe(
            TASK_SYNC_MAIN,
            GoalId::new(TASK_SYNC_MAIN),
            recipe.clone(),
            backend.clone(),
        ));
        let finalize = Arc::new(BackendInvokeTask::from_recipe(
            TASK_FINALIZE,
            GoalId::new(TASK_FINALIZE),
            recipe,
            backend,
        ));
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("merge_pr_workflow")
            .add_task(analyze)
            .add_task(sync)
            .add_task(finalize)
            .add_task(end)
            .add_edge(TASK_ANALYZE, TASK_SYNC_MAIN)
            .add_edge(TASK_SYNC_MAIN, TASK_FINALIZE)
            .add_edge(TASK_FINALIZE, "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(MergePrWorkflowHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            TASK_ANALYZE => Some(GoalHints {
                display_name: "Analyze merge feasibility".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: permissions::evaluate_allowlist(),
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            TASK_SYNC_MAIN | TASK_FINALIZE => Some(GoalHints {
                display_name: match goal_id.as_str() {
                    TASK_SYNC_MAIN => "Sync with main".to_string(),
                    _ => "Finalize merge / push".to_string(),
                },
                permission: PermissionHint::AcceptEdits,
                allowed_tools: permissions::merge_pr_allowlist(),
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            _ => None,
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![
            GoalId::new(TASK_ANALYZE),
            GoalId::new(TASK_SYNC_MAIN),
            GoalId::new(TASK_FINALIZE),
        ]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        match goal_id.as_str() {
            TASK_FINALIZE => GoalId::new(MERGE_PR_REPORT_GOAL),
            TASK_ANALYZE => GoalId::new(MERGE_PR_ANALYZE_GOAL),
            _ => goal_id.clone(),
        }
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Analyze" => Some(GoalId::new(TASK_ANALYZE)),
            "SyncMain" => Some(GoalId::new(TASK_SYNC_MAIN)),
            "Finalize" => Some(GoalId::new(TASK_FINALIZE)),
            "Failed" => None,
            _ => Some(GoalId::new(TASK_ANALYZE)),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Analyze")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new(TASK_ANALYZE)
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new(TASK_FINALIZE)
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        BTreeMap::new()
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool {
        matches!(
            goal_id.as_str(),
            TASK_ANALYZE | TASK_SYNC_MAIN | TASK_FINALIZE
        )
    }

    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &Path,
    ) -> Result<(), String> {
        log::info!(
            "MergePrRecipe::plain_goal_cli_output goal={} session_dir={}",
            goal_id.as_str(),
            session_dir.display()
        );
        if let Some(o) = output {
            log::info!("[merge-pr:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        matches!(goal_id.as_str(), TASK_FINALIZE)
    }
}

impl SessionArtifactManifest for MergePrRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[("merge_pr", MERGE_PR_REPORT_BASENAME)]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("merge_pr".to_string(), MERGE_PR_REPORT_BASENAME.to_string());
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}
