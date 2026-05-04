//! Shipped **TDD product workflow** (PRD/plan approval, red/green, demo branching, docs) — not generic engine glue.
//! Orchestration lives here; `tddy-core` stays recipe-neutral.

pub mod graph;
pub mod hooks;
pub(crate) mod hooks_common;
pub mod interview;
pub mod plan_task;
pub(crate) mod session_dir_resolve;

pub use hooks::TddWorkflowHooks;
pub use plan_task::PlanTask;
mod acceptance_tests;
mod acceptance_tests_action_templates;
mod demo;
mod evaluate;
pub(crate) mod green;
mod plain_cli_output;
mod planning;
pub mod red;
pub(crate) mod refactor;
pub(crate) mod update_docs;
mod validate_subagents;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::SessionArtifactManifest;
use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::workflow::graph::Graph;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};

/// Default TDD workflow (feature development with plan → acceptance tests → red/green → …).
#[derive(Clone, Copy, Default, Debug)]
pub struct TddRecipe;

impl TddRecipe {
    /// Human-readable goal label (matches former `Goal` display / CLI UX).
    fn display_name_for_goal(goal_id: &str) -> &'static str {
        match goal_id {
            "interview" => "Interview",
            "plan" => "Plan",
            "acceptance-tests" => "Acceptance tests",
            "red" => "Red",
            "green" => "Green",
            "demo" => "Demo",
            "evaluate" => "Evaluate",
            "validate" => "Validate",
            "refactor" => "Refactor",
            "update-docs" => "Update docs",
            _ => "Unknown",
        }
    }

    fn goal_hints_inner(&self, goal_id: &GoalId) -> Option<GoalHints> {
        let tools = |f: fn() -> Vec<String>| f();
        let is_plan = goal_id.as_str() == "plan";
        let (permission, allowed_tools) = match goal_id.as_str() {
            "interview" => (
                PermissionHint::ReadOnly,
                tools(crate::permissions::plan_allowlist),
            ),
            "plan" => (
                PermissionHint::ReadOnly,
                tools(crate::permissions::plan_allowlist),
            ),
            "acceptance-tests" | "red" | "green" => (
                PermissionHint::AcceptEdits,
                tools(crate::permissions::acceptance_tests_allowlist),
            ),
            "demo" => (
                PermissionHint::AcceptEdits,
                tools(crate::permissions::demo_allowlist),
            ),
            "evaluate" => (
                PermissionHint::ReadOnly,
                tools(crate::permissions::evaluate_allowlist),
            ),
            "validate" => (
                PermissionHint::ReadOnly,
                tools(crate::permissions::validate_subagents_allowlist),
            ),
            "refactor" => (
                PermissionHint::AcceptEdits,
                tools(crate::permissions::refactor_allowlist),
            ),
            "update-docs" => (
                PermissionHint::AcceptEdits,
                tools(crate::permissions::update_docs_allowlist),
            ),
            _ => return None,
        };
        Some(GoalHints {
            display_name: Self::display_name_for_goal(goal_id.as_str()).to_string(),
            permission,
            allowed_tools,
            default_model: None,
            agent_output: matches!(
                goal_id.as_str(),
                "interview" | "green" | "red" | "acceptance-tests" | "demo"
            ),
            agent_cli_plan_mode: is_plan,
            claude_nonzero_exit_ok_if_structured_response: is_plan,
        })
    }

    fn next_goal_for_state_inner(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "Interview" | "Interviewing" => Some(GoalId::new("interview")),
            "Interviewed" | "Planning" => Some(GoalId::new("plan")),
            "Planned" | "AcceptanceTesting" => Some(GoalId::new("acceptance-tests")),
            "AcceptanceTestsReady" | "RedTesting" => Some(GoalId::new("red")),
            "RedTestsReady" | "GreenImplementing" => Some(GoalId::new("green")),
            "GreenComplete" | "DemoRunning" => Some(GoalId::new("demo")),
            "DemoComplete" | "Evaluating" => Some(GoalId::new("evaluate")),
            "Evaluated" | "Validating" => Some(GoalId::new("validate")),
            "ValidateComplete" | "ValidateRefactorComplete" | "Refactoring" => {
                Some(GoalId::new("refactor"))
            }
            "RefactorComplete" | "UpdatingDocs" => Some(GoalId::new("update-docs")),
            "DocsUpdated" | "Failed" => None,
            _ => Some(GoalId::new("interview")),
        }
    }

    fn status_for_state_inner(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "Init" | "Interview" | "Planned" | "AcceptanceTestsReady" | "RedTestsReady" => "Active",
            "GreenComplete"
            | "DemoComplete"
            | "Evaluated"
            | "ValidateComplete"
            | "ValidateRefactorComplete"
            | "RefactorComplete"
            | "DocsUpdated" => "Completed",
            "Failed" => "Failed",
            _ => "Active",
        }
    }
}

impl WorkflowRecipe for TddRecipe {
    fn name(&self) -> &str {
        "tdd"
    }

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        graph::build_full_tdd_workflow_graph(backend, recipe)
    }

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(*self);
        let manifest: Arc<dyn SessionArtifactManifest> = Arc::new(*self);
        Arc::new(hooks::TddWorkflowHooks::with_event_tx_optional(
            recipe, manifest, event_tx,
        ))
    }

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints> {
        self.goal_hints_inner(goal_id)
    }

    fn goal_ids(&self) -> Vec<GoalId> {
        [
            "interview",
            "plan",
            "acceptance-tests",
            "red",
            "green",
            "demo",
            "evaluate",
            "validate",
            "refactor",
            "update-docs",
        ]
        .into_iter()
        .map(GoalId::new)
        .collect()
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        if goal_id.as_str() == "evaluate" {
            GoalId::new("evaluate-changes")
        } else {
            goal_id.clone()
        }
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        self.next_goal_for_state_inner(state)
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        self.status_for_state_inner(state)
    }

    fn initial_state(&self) -> WorkflowState {
        WorkflowState::new("Interview")
    }

    fn start_goal(&self) -> GoalId {
        GoalId::new("interview")
    }

    fn plan_refinement_goal(&self) -> GoalId {
        GoalId::new("plan")
    }

    fn default_models(&self) -> BTreeMap<GoalId, String> {
        let mut m = BTreeMap::new();
        m.insert(GoalId::new("interview"), "sonnet".to_string());
        m.insert(GoalId::new("plan"), "opus".to_string());
        for g in ["acceptance-tests", "red", "green", "demo"] {
            m.insert(GoalId::new(g), "sonnet".to_string());
        }
        m
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        !matches!(goal_id.as_str(), "interview")
    }

    fn skip_failed_resume_transition(
        &self,
        transition_state: &WorkflowState,
        next_goal: &GoalId,
    ) -> bool {
        if next_goal == &self.start_goal() {
            return true;
        }
        transition_state.as_str() == "Planning" && next_goal.as_str() == "plan"
    }

    fn uses_primary_session_document(&self) -> bool {
        true
    }

    fn read_primary_session_document_utf8(&self, session_dir: &Path) -> Option<String> {
        self.primary_document_basename()
            .and_then(|b| tddy_workflow::read_session_artifact_utf8(session_dir, &b))
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool {
        !matches!(goal_id.as_str(), "plan")
    }

    fn summarize_last_goal_output(&self, raw_output: &str) -> Option<String> {
        use crate::parser::{parse_refactor_response, parse_update_docs_response};
        parse_update_docs_response(raw_output)
            .ok()
            .map(|r| format!("Docs updated: {}", r.docs_updated))
            .or_else(|| {
                parse_refactor_response(raw_output).ok().map(|r| {
                    format!(
                        "Tasks completed: {}\nTests passing: {}",
                        r.tasks_completed, r.tests_passing
                    )
                })
            })
    }

    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &std::path::Path,
    ) -> Result<(), String> {
        plain_cli_output::print_plain_goal_output(goal_id, output, session_dir)
    }
}

impl SessionArtifactManifest for TddRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("prd", "PRD.md"),
            ("acceptance_tests", "acceptance-tests.md"),
            ("progress", "progress.md"),
            ("red_output", "red-output.md"),
            ("evaluation_report", "evaluation-report.md"),
            ("demo_plan", "demo-plan.md"),
            ("demo_results", "demo-results.md"),
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
        a.insert("demo_plan".to_string(), "demo-plan.md".to_string());
        a.insert("demo_results".to_string(), "demo-results.md".to_string());
        a.insert(
            "system_prompt_plan".to_string(),
            "system-prompt-plan.md".to_string(),
        );
        a
    }
}

#[cfg(test)]
mod planning_intent_tests {
    use super::TddRecipe;
    use tddy_core::GoalId;
    use tddy_core::WorkflowRecipe;

    /// Backends must use [`tddy_core::backend::GoalHints::agent_cli_plan_mode`] (not goal id) for vendor plan-mode CLI flags.
    #[test]
    fn agent_cli_plan_mode_is_true_only_for_plan_goal() {
        let r = TddRecipe;
        assert!(
            r.goal_hints(&GoalId::new("plan"))
                .unwrap()
                .agent_cli_plan_mode
        );
        assert!(
            !r.goal_hints(&GoalId::new("evaluate"))
                .unwrap()
                .agent_cli_plan_mode
        );
        assert!(
            !r.goal_hints(&GoalId::new("validate"))
                .unwrap()
                .agent_cli_plan_mode
        );
        assert!(
            !r.goal_hints(&GoalId::new("red"))
                .unwrap()
                .agent_cli_plan_mode
        );
        assert!(
            !r.goal_hints(&GoalId::new("interview"))
                .unwrap()
                .agent_cli_plan_mode
        );
    }

    /// Plan refinement (PRD feedback) must target **`plan`**, not the entry **`interview`** step.
    #[test]
    fn plan_refinement_goal_is_plan_not_interview() {
        let r = TddRecipe;
        assert_eq!(r.plan_refinement_goal(), GoalId::new("plan"));
        assert_ne!(r.plan_refinement_goal(), r.start_goal());
    }
}
