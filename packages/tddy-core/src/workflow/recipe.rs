//! Pluggable workflow definitions (`WorkflowRecipe`).

use crate::backend::{ClarificationQuestion, CodingBackend};
use crate::presenter::WorkflowEvent;
use crate::workflow::context::Context;
use crate::workflow::graph::Graph;
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::ids::{GoalId, WorkflowState};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Channel type for TUI workflow events from hooks.
pub type WorkflowEventSender = Sender<WorkflowEvent>;

/// Backend-agnostic permission hint (mapped per backend in claude/cursor/acp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionHint {
    ReadOnly,
    AcceptEdits,
}

/// Per-goal configuration for backends (replaces matching on a `Goal` enum).
#[derive(Debug, Clone)]
pub struct GoalHints {
    pub display_name: String,
    pub permission: PermissionHint,
    pub allowed_tools: Vec<String>,
    pub default_model: Option<String>,
    pub agent_output: bool,
    /// When true, backends enable vendor “plan mode” CLI flags (e.g. Cursor `--plan`, Claude `--permission-mode plan`).
    pub agent_cli_plan_mode: bool,
    /// Claude CLI: if the process exits non-zero but stdout contains `<structured-response`, treat as success.
    /// Set by the recipe for goals that emit structured JSON despite a non-zero exit code.
    pub claude_nonzero_exit_ok_if_structured_response: bool,
}

/// Open–closed plugin: each workflow (TDD, bug-fix, …) implements this in `tddy-workflow-recipes`.
pub trait WorkflowRecipe: Send + Sync {
    fn name(&self) -> &str;

    fn build_graph(&self, backend: Arc<dyn CodingBackend>) -> Graph;

    fn create_hooks(&self, event_tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks>;

    fn goal_hints(&self, goal_id: &GoalId) -> Option<GoalHints>;

    fn goal_ids(&self) -> Vec<GoalId>;

    /// JSON / tool-submit channel key for this goal (may differ from graph task id, e.g. evaluate vs evaluate-changes).
    fn submit_key(&self, goal_id: &GoalId) -> GoalId;

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId>;

    /// Coarse status for sessions (e.g. `"Active"`, `"Completed"`, `"Failed"`).
    fn status_for_state(&self, state: &WorkflowState) -> &'static str;

    fn initial_state(&self) -> WorkflowState;

    fn start_goal(&self) -> GoalId;

    /// Goal to run for **plan refinement** (PRD feedback after primary document review).
    ///
    /// Defaults to [`start_goal`](Self::start_goal). Recipes with a dedicated planning step after
    /// elicitation must override (e.g. TDD **`plan`** after **`interview`**, grill-me **`create-plan`**
    /// after **`grill`**).
    fn plan_refinement_goal(&self) -> GoalId {
        self.start_goal()
    }

    fn default_models(&self) -> BTreeMap<GoalId, String>;

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool;

    /// Whether this workflow expects a primary on-disk session document during the start phase (approval gate, resume checks).
    fn uses_primary_session_document(&self) -> bool {
        false
    }

    /// UTF-8 content for session document approval / review UI, if the recipe defines one on disk.
    /// Default: none (workflows without a primary session document).
    fn read_primary_session_document_utf8(&self, _session_dir: &Path) -> Option<String> {
        None
    }

    /// Optional structured summary of the last goal's `tddy-tools submit` output (e.g. update-docs vs refactor).
    /// Used by the presenter workflow thread for the completion message; recipes that use JSON outputs implement this.
    fn summarize_last_goal_output(&self, raw_output: &str) -> Option<String> {
        let _ = raw_output;
        None
    }

    /// Plain CLI (`--goal`): print human-readable lines after a single goal run from structured agent output.
    /// Recipes own parsers and formatting (e.g. acceptance-tests, red, green).
    fn plain_goal_cli_output(
        &self,
        goal_id: &GoalId,
        output: Option<&str>,
        session_dir: &Path,
    ) -> Result<(), String>;

    /// When `false`, [`crate::workflow::task::BackendInvokeTask`] may complete a turn from agent output
    /// alone (no `tddy-tools submit`), e.g. open-ended chat goals. Default `true` preserves structured
    /// submit for TDD-style goals.
    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        let _ = goal_id;
        true
    }

    /// Whether to ignore this history transition when choosing a resume goal after `Failed`
    /// ([`crate::changeset::start_goal_for_session_continue`]). Default: skip when the computed
    /// next goal equals [`start_goal`](WorkflowRecipe::start_goal). Recipes with a pre-plan step may
    /// override (e.g. TDD skips `Planning` → `plan` as restart noise even when `start_goal` is `interview`).
    fn skip_failed_resume_transition(
        &self,
        transition_state: &WorkflowState,
        next_goal: &GoalId,
    ) -> bool {
        let _ = transition_state;
        next_goal == &self.start_goal()
    }

    /// After a no-submit backend turn, optional host clarification before advancing (e.g. grill-me
    /// confirming the user is ready for **Create plan** when `tddy-tools ask` did not persist answers).
    fn host_clarification_gate_after_no_submit_turn(
        &self,
        goal_id: &GoalId,
        context: &Context,
    ) -> Option<Vec<ClarificationQuestion>> {
        let _ = (goal_id, context);
        None
    }
}
