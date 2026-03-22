//! Pluggable workflow definitions (`WorkflowRecipe`).

use crate::backend::CodingBackend;
use crate::presenter::WorkflowEvent;
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
    /// Planning-workflow intent (PRD/plan phase), set by [`WorkflowRecipe::goal_hints`].
    /// Backends map this without inspecting goal id: Cursor adds `agent --plan`; Claude adds
    /// `--permission-mode plan` only when this is true (read-only goals without this flag use default mode).
    pub planning_mode_intent: bool,
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

    fn default_models(&self) -> BTreeMap<GoalId, String>;

    fn default_artifacts(&self) -> BTreeMap<String, String>;

    fn known_artifacts(&self) -> &[(&'static str, &'static str)];

    /// Basenames under the session directory to consider when building `<context-reminder>` artifact lines (existing files only).
    /// Default: filenames from [`Self::known_artifacts`].
    fn context_header_session_artifact_filenames(&self) -> Vec<&'static str> {
        self.known_artifacts()
            .iter()
            .map(|(_, name)| *name)
            .collect()
    }

    fn goal_requires_session_dir(&self, goal_id: &GoalId) -> bool;

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
}
