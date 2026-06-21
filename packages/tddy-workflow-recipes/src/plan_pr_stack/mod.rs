//! **plan-pr-stack** workflow: analyze feature intent, then emit a structured PR-stack plan.

mod hooks;
mod prompt;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub use hooks::PlanPrStackHooks;
pub use prompt::{STACK_PLAN_BASENAME, PR_STACK_PLAN_MD_BASENAME};

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};
use tddy_core::StackNode;

use crate::SessionArtifactManifest;

/// Structured output the agent must emit for `write-stack-plan` goal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StackPlanOutput {
    pub version: u32,
    pub prs: Vec<PlannedPr>,
}

/// One PR entry in the plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlannedPr {
    pub node_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_suggestion: Option<String>,
    #[serde(default)]
    pub parents: Vec<String>,
    /// Optional workflow recipe to use for this child (e.g. "tdd", "bugfix"). Default: "tdd".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_recipe: Option<String>,
}

/// Convert a parsed plan into StackNode list (no child session yet — session_id/branch = None).
pub fn planned_prs_into_stack_nodes(prs: &[PlannedPr]) -> Vec<StackNode> {
    unimplemented!("planned_prs_into_stack_nodes: not yet implemented")
}

/// Validate a StackPlanOutput: unique node_ids, all parents resolve, no cycle.
pub fn validate_stack_plan(plan: &StackPlanOutput) -> Result<(), String> {
    unimplemented!("validate_stack_plan: not yet implemented")
}

/// **plan-pr-stack** recipe: `analyze-stack` → `write-stack-plan` → `end`.
#[derive(Clone, Copy, Default, Debug)]
pub struct PlanPrStackRecipe;

impl WorkflowRecipe for PlanPrStackRecipe {
    fn name(&self) -> &str { "plan-pr-stack" }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let analyze = Arc::new(BackendInvokeTask::from_recipe(
            "analyze-stack", GoalId::new("analyze-stack"), recipe.clone(), backend.clone(),
        ));
        let write_plan = Arc::new(BackendInvokeTask::from_recipe(
            "write-stack-plan", GoalId::new("write-stack-plan"), recipe, backend,
        ));
        let end = Arc::new(EndTask::new("end"));
        GraphBuilder::new("plan_pr_stack")
            .add_task(analyze)
            .add_task(write_plan)
            .add_task(end)
            .add_edge("analyze-stack", "write-stack-plan")
            .add_edge("write-stack-plan", "end")
            .build()
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        Arc::new(PlanPrStackHooks::new(event_tx))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        match goal_id.as_str() {
            "analyze-stack" => Some(GoalHints {
                display_name: "Analyze stack".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: true,
                claude_nonzero_exit_ok_if_structured_response: false,
            }),
            "write-stack-plan" => Some(GoalHints {
                display_name: "Write stack plan".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: true,
            }),
            _ => None,
        }
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        vec![GoalId::new("analyze-stack"), GoalId::new("write-stack-plan")]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId { goal_id.clone() }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "AnalyzeStack" => Some(GoalId::new("analyze-stack")),
            "WriteStackPlan" => Some(GoalId::new("write-stack-plan")),
            "Failed" | "StackPlanned" => None,
            _ => Some(GoalId::new("analyze-stack")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Failed" => "Failed",
            "StackPlanned" => "Completed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState { WorkflowState::new("AnalyzeStack") }

    fn start_goal(&self) -> GoalId { GoalId::new("analyze-stack") }

    fn plan_refinement_goal(&self) -> GoalId { GoalId::new("write-stack-plan") }

    fn default_models(&self) -> BTreeMap<GoalId, String> { BTreeMap::new() }

    fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool { true }

    fn uses_primary_session_document(&self) -> bool { false }

    fn plain_goal_cli_output(&self, goal_id: &GoalId, output: Option<&str>, _session_dir: &Path) -> Result<(), String> {
        if let Some(o) = output {
            log::info!("[plan-pr-stack:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        goal_id.as_str() == "write-stack-plan"
    }
}

impl SessionArtifactManifest for PlanPrStackRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("stack_plan", STACK_PLAN_BASENAME),
            ("stack_plan_md", PR_STACK_PLAN_MD_BASENAME),
        ]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("stack_plan".to_string(), STACK_PLAN_BASENAME.to_string());
        a.insert("stack_plan_md".to_string(), PR_STACK_PLAN_MD_BASENAME.to_string());
        a
    }

    fn primary_document_basename(&self) -> Option<String> { None }
}
