//! `TddSmallRecipe` — [`WorkflowRecipe`] + [`SessionArtifactManifest`] for `tdd-small`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::Graph;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};

use crate::tdd_small::graph::build_tdd_small_workflow_graph;
use crate::tdd_small::hooks::TddSmallWorkflowHooks;
use crate::SessionArtifactManifest;

/// Shipped **tdd-small** workflow (shortened TDD graph).
#[derive(Clone, Copy, Default, Debug)]
pub struct TddSmallRecipe;

impl TddSmallRecipe {
    fn display_name_for_goal(goal_id: &str) -> &'static str {
        match goal_id {
            "plan" => "Plan",
            "red" => "Red",
            "green" => "Green",
            "post-green-review" => "Post-green review",
            "refactor" => "Refactor",
            "update-docs" => "Update docs",
            _ => "Unknown",
        }
    }
}

impl WorkflowRecipe for TddSmallRecipe {
    fn name(&self) -> &str {
        "tdd-small"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        log::info!("TddSmallRecipe::build_graph: constructing tdd-small workflow graph");
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        build_tdd_small_workflow_graph(backend, recipe)
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let manifest: Arc<dyn SessionArtifactManifest> = Arc::new(*self);
        Arc::new(TddSmallWorkflowHooks::with_event_tx_optional(
            recipe, manifest, event_tx,
        ))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        let is_plan = goal_id.as_str() == "plan";
        let (permission, allowed_tools): (PermissionHint, Vec<String>) = match goal_id.as_str() {
            "plan" => (
                PermissionHint::ReadOnly,
                crate::permissions::plan_allowlist(),
            ),
            "red" | "green" => (
                PermissionHint::AcceptEdits,
                crate::permissions::acceptance_tests_allowlist(),
            ),
            "post-green-review" => (
                PermissionHint::ReadOnly,
                crate::permissions::evaluate_allowlist(),
            ),
            "refactor" => (
                PermissionHint::AcceptEdits,
                crate::permissions::refactor_allowlist(),
            ),
            "update-docs" => (
                PermissionHint::AcceptEdits,
                crate::permissions::update_docs_allowlist(),
            ),
            _ => return None,
        };
        Some(GoalHints {
            display_name: Self::display_name_for_goal(goal_id.as_str()).to_string(),
            permission,
            allowed_tools,
            default_model: None,
            agent_output: matches!(goal_id.as_str(), "green" | "red"),
            agent_cli_plan_mode: is_plan,
            claude_nonzero_exit_ok_if_structured_response: is_plan,
        })
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        [
            "plan",
            "red",
            "green",
            "post-green-review",
            "refactor",
            "update-docs",
        ]
        .into_iter()
        .map(GoalId::new)
        .collect()
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        if goal_id.as_str() == "post-green-review" {
            GoalId::new("post-green-review")
        } else {
            goal_id.clone()
        }
    }

    /// Next goal for resume / UI given `changeset.yaml` workflow state.
    ///
    /// **Legacy state names** (`AcceptanceTesting`, `AcceptanceTestsReady`, `DemoRunning`, `Evaluating`,
    /// `Evaluated`, `Validating`, …) are included so sessions that migrated from full **`tdd`** or mixed
    /// histories still resolve to a sensible next step on the **tdd-small** graph (there is no separate
    /// `acceptance-tests` or `demo` task; merged `red` and `post-green-review` cover that work).
    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        log::debug!(
            "TddSmallRecipe::next_goal_for_state: current={}",
            state.as_str()
        );
        match state.as_str() {
            "Init" | "Planning" => Some(GoalId::new("plan")),
            // Merged recipe: after plan we go straight to merged red (no standalone acceptance-tests task).
            "Planned" | "AcceptanceTesting" => Some(GoalId::new("red")),
            "AcceptanceTestsReady" | "RedTesting" => Some(GoalId::new("red")),
            "RedTestsReady" | "GreenImplementing" => Some(GoalId::new("green")),
            // No demo branch: green completes straight into merged post-green review.
            "GreenComplete" | "DemoRunning" => Some(GoalId::new("post-green-review")),
            "Evaluating" | "Evaluated" => Some(GoalId::new("post-green-review")),
            "Validating" | "ValidateComplete" | "ValidateRefactorComplete" => {
                Some(GoalId::new("refactor"))
            }
            "Refactoring" => Some(GoalId::new("refactor")),
            "RefactorComplete" | "UpdatingDocs" => Some(GoalId::new("update-docs")),
            "DocsUpdated" | "Failed" => None,
            _ => Some(GoalId::new("plan")),
        }
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Init" | "Planned" | "AcceptanceTestsReady" | "RedTestsReady" => "Active",
            "GreenComplete"
            | "Evaluated"
            | "ValidateComplete"
            | "ValidateRefactorComplete"
            | "RefactorComplete"
            | "DocsUpdated" => "Completed",
            "Failed" => "Failed",
            _ => "Active",
        }
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Init")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("plan")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        let mut m = BTreeMap::new();
        m.insert(GoalId::new("plan"), "opus".to_string());
        for g in [
            "red",
            "green",
            "post-green-review",
            "refactor",
            "update-docs",
        ] {
            m.insert(GoalId::new(g), "sonnet".to_string());
        }
        m
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool {
        goal_id.as_str() != "plan"
    }

    fn uses_primary_session_document(&self) -> bool {
        true
    }

    fn read_primary_session_document_utf8(&self, session_dir: &Path) -> Option<String> {
        self.primary_document_basename()
            .and_then(|b| tddy_workflow::read_session_artifact_utf8(session_dir, &b))
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

impl SessionArtifactManifest for TddSmallRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("prd", "PRD.md"),
            ("acceptance_tests", "acceptance-tests.md"),
            ("progress", "progress.md"),
            ("red_output", "red-output.md"),
            ("evaluation_report", "evaluation-report.md"),
            ("validate_tests", "validate-tests-report.md"),
            ("validate_prod_ready", "validate-prod-ready-report.md"),
            ("analyze_clean_code", "analyze-clean-code-report.md"),
            ("refactoring_plan", "refactoring-plan.md"),
            ("update_docs", "update-docs-report.md"),
        ]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("prd".to_string(), "PRD.md".to_string());
        a.insert(
            "acceptance_tests".to_string(),
            "acceptance-tests.md".to_string(),
        );
        a.insert("red_output".to_string(), "red-output.md".to_string());
        a.insert("progress".to_string(), "progress.md".to_string());
        a.insert(
            "system_prompt_plan".to_string(),
            "system-prompt-plan.md".to_string(),
        );
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::WorkflowRecipe;

    #[test]
    fn tdd_small_goal_ids_includes_post_green_review() {
        let r = TddSmallRecipe;
        let ids: Vec<String> = r
            .goal_ids()
            .into_iter()
            .map(|g| g.as_str().to_string())
            .collect();
        assert!(
            ids.contains(&"post-green-review".to_string()),
            "expected post-green-review in goal_ids, got {:?}",
            ids
        );
    }
}
