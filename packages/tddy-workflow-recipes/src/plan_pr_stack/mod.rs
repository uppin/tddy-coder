//! **plan-pr-stack** workflow: analyze feature intent, then emit a structured PR-stack plan.

mod hooks;
mod prompt;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub use hooks::PlanPrStackHooks;
pub use prompt::{
    analyze_stack_user_prompt, write_stack_plan_user_prompt, PR_STACK_PLAN_MD_BASENAME,
    STACK_PLAN_BASENAME,
};

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
    /// Optional code-discovery map (markdown, with `path:line` references) persisted to
    /// `artifacts/exploration.md`, mirroring the tdd/bugfix planning recipes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exploration: Option<String>,
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
    prs.iter()
        .map(|pr| StackNode {
            node_id: pr.node_id.clone(),
            title: pr.title.clone(),
            description: pr.description.clone(),
            branch_suggestion: pr.branch_suggestion.clone(),
            branch: None,
            session_id: None,
            parents: pr.parents.clone(),
            pr_status: None,
            child_state: None,
            internal_status: None,
        })
        .collect()
}

/// Validate a StackPlanOutput: unique node_ids, all parents resolve, no cycle.
pub fn validate_stack_plan(plan: &StackPlanOutput) -> Result<(), String> {
    use std::collections::HashSet;

    // 1. Duplicate node_ids.
    let mut seen: HashSet<&str> = HashSet::new();
    for pr in &plan.prs {
        if !seen.insert(pr.node_id.as_str()) {
            return Err(format!("duplicate node_id: {}", pr.node_id));
        }
    }

    // 2. Dangling parent refs.
    for pr in &plan.prs {
        for parent in &pr.parents {
            if !seen.contains(parent.as_str()) {
                return Err(format!(
                    "dangling parent ref: {parent} in node {}",
                    pr.node_id
                ));
            }
        }
    }

    // 3. Cycle detection via topological sort.
    let stack = tddy_core::changeset::Stack {
        version: plan.version,
        nodes: planned_prs_into_stack_nodes(&plan.prs),
    };
    stack
        .topo_order()
        .map_err(|err| format!("cycle detected: {err}"))?;

    // 4. Branch-name contract: every PR carries a `branch_suggestion` in `feature/<stack>/<node>`
    // form, and all PRs share one `feature/<stack>/` namespace so the stack's branches group
    // together (and "Start session" always has a non-empty new_branch_name). See
    // docs/ft/coder/pr-stacking.md.
    let mut stack_namespace: Option<String> = None;
    for pr in &plan.prs {
        let branch = pr
            .branch_suggestion
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .ok_or_else(|| format!("missing branch_suggestion for node {}", pr.node_id))?;
        let namespace = branch_stack_namespace(branch).ok_or_else(|| {
            format!(
                "branch_suggestion {branch:?} for node {} must be in feature/<stack>/<node> form",
                pr.node_id
            )
        })?;
        match &stack_namespace {
            None => stack_namespace = Some(namespace),
            Some(existing) if *existing != namespace => {
                return Err(format!(
                    "branches must share one stack namespace: {existing} vs {namespace} (node {})",
                    pr.node_id
                ));
            }
            Some(_) => {}
        }
    }

    Ok(())
}

/// The `feature/<stack>/` namespace of a stacked-PR branch, or `None` when the branch is not in
/// `feature/<stack>/<node>` form (at least three non-empty `/`-separated segments led by `feature`).
/// Used to group every PR's branch under one shared stack namespace.
fn branch_stack_namespace(branch: &str) -> Option<String> {
    let segments: Vec<&str> = branch.split('/').collect();
    if segments.len() >= 3 && segments[0] == "feature" && segments.iter().all(|s| !s.is_empty()) {
        Some(format!("feature/{}", segments[1]))
    } else {
        None
    }
}

/// **plan-pr-stack** recipe: `analyze-stack` → `write-stack-plan` → `end`.
#[derive(Clone, Copy, Default, Debug)]
pub struct PlanPrStackRecipe;

impl WorkflowRecipe for PlanPrStackRecipe {
    fn name(&self) -> &str {
        "plan-pr-stack"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let analyze = Arc::new(BackendInvokeTask::from_recipe(
            "analyze-stack",
            GoalId::new("analyze-stack"),
            recipe.clone(),
            backend.clone(),
        ));
        let write_plan = Arc::new(BackendInvokeTask::from_recipe(
            "write-stack-plan",
            GoalId::new("write-stack-plan"),
            recipe,
            backend,
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
        vec![
            GoalId::new("analyze-stack"),
            GoalId::new("write-stack-plan"),
        ]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

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

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("AnalyzeStack")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("analyze-stack")
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new("write-stack-plan")
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
        a.insert(
            "stack_plan_md".to_string(),
            PR_STACK_PLAN_MD_BASENAME.to_string(),
        );
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_happy_path_three_node_dag() {
        let plan = StackPlanOutput {
            version: 1,
            exploration: None,
            prs: vec![
                PlannedPr {
                    node_id: "n1".to_string(),
                    title: "Auth token store".to_string(),
                    description: "Store tokens securely".to_string(),
                    branch_suggestion: Some("feature/auth-store".to_string()),
                    parents: vec![],
                    child_recipe: None,
                },
                PlannedPr {
                    node_id: "n2".to_string(),
                    title: "Auth middleware".to_string(),
                    description: "Validate tokens".to_string(),
                    branch_suggestion: Some("feature/auth-middleware".to_string()),
                    parents: vec!["n1".to_string()],
                    child_recipe: None,
                },
                PlannedPr {
                    node_id: "n3".to_string(),
                    title: "Auth UI".to_string(),
                    description: "Login page".to_string(),
                    branch_suggestion: Some("feature/auth-ui".to_string()),
                    parents: vec!["n1".to_string(), "n2".to_string()],
                    child_recipe: Some("tdd".to_string()),
                },
            ],
        };

        let nodes = planned_prs_into_stack_nodes(&plan.prs);
        assert_eq!(nodes.len(), 3);

        let n1 = nodes.iter().find(|n| n.node_id == "n1").unwrap();
        assert!(n1.parents.is_empty());
        assert_eq!(n1.branch_suggestion.as_deref(), Some("feature/auth-store"));
        assert!(n1.session_id.is_none());
        assert!(n1.branch.is_none());

        let n2 = nodes.iter().find(|n| n.node_id == "n2").unwrap();
        assert_eq!(n2.parents, vec!["n1".to_string()]);

        let n3 = nodes.iter().find(|n| n.node_id == "n3").unwrap();
        assert_eq!(n3.parents, vec!["n1".to_string(), "n2".to_string()]);
    }

    #[test]
    fn validate_stack_plan_rejects_duplicate_node_id() {
        let plan = StackPlanOutput {
            version: 1,
            exploration: None,
            prs: vec![
                PlannedPr {
                    node_id: "n1".to_string(),
                    title: "First".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    parents: vec![],
                    child_recipe: None,
                },
                PlannedPr {
                    node_id: "n1".to_string(),
                    title: "Duplicate".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    parents: vec![],
                    child_recipe: None,
                },
            ],
        };
        let result = validate_stack_plan(&plan);
        assert!(result.is_err(), "expected Err for duplicate node_id");
        let msg = result.unwrap_err().to_lowercase();
        assert!(
            msg.contains("duplicate") || msg.contains("n1"),
            "error should mention duplicate or node id, got: {msg}"
        );
    }

    #[test]
    fn validate_stack_plan_rejects_dangling_parent_ref() {
        let plan = StackPlanOutput {
            version: 1,
            exploration: None,
            prs: vec![PlannedPr {
                node_id: "n2".to_string(),
                title: "Orphan".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n1".to_string()], // n1 does not exist
                child_recipe: None,
            }],
        };
        let result = validate_stack_plan(&plan);
        assert!(result.is_err(), "expected Err for dangling parent ref");
        let msg = result.unwrap_err().to_lowercase();
        assert!(
            msg.contains("n1") || msg.contains("dangling") || msg.contains("parent"),
            "error should mention the missing parent, got: {msg}"
        );
    }

    #[test]
    fn validate_stack_plan_rejects_cycle() {
        let plan = StackPlanOutput {
            version: 1,
            exploration: None,
            prs: vec![
                PlannedPr {
                    node_id: "n1".to_string(),
                    title: "A".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    parents: vec!["n2".to_string()],
                    child_recipe: None,
                },
                PlannedPr {
                    node_id: "n2".to_string(),
                    title: "B".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    parents: vec!["n1".to_string()],
                    child_recipe: None,
                },
            ],
        };
        let result = validate_stack_plan(&plan);
        assert!(result.is_err(), "expected Err for cycle in plan");
        let msg = result.unwrap_err().to_lowercase();
        assert!(
            msg.contains("cycle"),
            "error should mention cycle, got: {msg}"
        );
    }

    #[test]
    fn stack_plan_output_deserializes_an_optional_exploration_field() {
        // Given — a write-stack-plan submission that carries a code-discovery exploration doc
        // alongside the PR list, so pr-stack can persist artifacts/exploration.md like the other
        // planning recipes (tdd/tdd-small/bugfix).
        let yaml = r##"version: 1
exploration: "# Exploration\n- src/lib.rs:1 entry point"
prs:
  - node_id: n1
    title: Root PR
    branch_suggestion: feature/auth/root
    parents: []
"##;

        // When
        let plan: StackPlanOutput = serde_yaml::from_str(yaml).expect("parse stack plan");

        // Then
        assert_eq!(
            plan.exploration.as_deref(),
            Some("# Exploration\n- src/lib.rs:1 entry point")
        );
    }

    #[test]
    fn stack_plan_output_without_exploration_leaves_it_none() {
        // Given — a submission that omits the exploration field entirely
        let yaml = "version: 1\nprs: []\n";

        // When
        let plan: StackPlanOutput = serde_yaml::from_str(yaml).expect("parse stack plan");

        // Then
        assert_eq!(plan.exploration, None);
    }

    #[test]
    fn planned_prs_into_stack_nodes_maps_all_fields() {
        let pr = PlannedPr {
            node_id: "n1".to_string(),
            title: "My PR".to_string(),
            description: "Does things".to_string(),
            branch_suggestion: Some("feature/my-pr".to_string()),
            parents: vec!["n0".to_string()],
            child_recipe: Some("tdd".to_string()),
        };
        let nodes = planned_prs_into_stack_nodes(&[pr]);
        assert_eq!(nodes.len(), 1);
        let n = &nodes[0];
        assert_eq!(n.node_id, "n1");
        assert_eq!(n.title, "My PR");
        assert_eq!(n.description, "Does things");
        assert_eq!(n.branch_suggestion.as_deref(), Some("feature/my-pr"));
        assert_eq!(n.parents, vec!["n0".to_string()]);
        assert!(n.session_id.is_none());
        assert!(n.branch.is_none());
        assert!(n.pr_status.is_none());
        assert!(n.child_state.is_none());
    }
}
