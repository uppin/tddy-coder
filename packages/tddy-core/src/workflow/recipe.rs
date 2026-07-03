//! Pluggable workflow definitions (`WorkflowRecipe`).

use crate::backend::{ClarificationQuestion, CodingBackend};
use crate::changeset::Changeset;
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

    /// Instruction text handed back to the agent when it transitions **into** `goal_id`
    /// (agent-driven orchestration, [`crate::workflow::controller::WorkflowController`]).
    ///
    /// In the single-session agent-driven model the agent already holds the earlier chat context
    /// (PRD, acceptance tests, …), so this returns the *static* per-goal instructions only — the
    /// same content each goal's `system_prompt()` provides in the engine-driven path. Recipes
    /// override to return rich per-goal instructions; the default is a generic proceed message so
    /// recipes not yet migrated to agent-driven mode still behave safely.
    fn goal_instructions(&self, goal_id: &GoalId) -> String {
        format!("Proceed with the '{}' goal.", goal_id)
    }

    /// System prompt for the **agent-driven orchestrator** session (single long-lived chat).
    ///
    /// Describes the workflow goals, the `transition` tool contract, and the subagent go/no-go
    /// protocol, then appends the current goal's instructions. Generic default composed from
    /// [`goal_ids`](Self::goal_ids), [`start_goal`](Self::start_goal), and
    /// [`goal_instructions`](Self::goal_instructions); recipes may override to tune wording.
    fn orchestration_system_prompt(&self, current: &GoalId) -> String {
        let goals = self
            .goal_ids()
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "You are the orchestrator for the '{name}' workflow. You drive a state machine yourself \
by calling the `tddy-tools transition` tool — there is no external driver.\n\
\n\
Workflow goals: [{goals}]\n\
You are currently at goal: '{current}'.\n\
\n\
Protocol:\n\
- Work on the current goal. When it is complete, advance by running:\n\
    tddy-tools transition --to <next-goal>\n\
  The tool returns the next goal's instructions as JSON (`instructions`). Read them and continue \
working in THIS same chat — do not start over.\n\
- Transitions must follow valid edges. If a transition is rejected, the tool's `reason` lists the \
valid next goals; pick one of those.\n\
- Subagents: you may spawn a subagent (via the Agent tool) to work a specific goal. Instruct each \
subagent, when it believes its goal is done, to call:\n\
    tddy-tools transition --to <goal> --provisional\n\
  A provisional transition is recorded but NOT committed. You (the orchestrator) MUST review the \
subagent's work and then make the go/no-go decision: to accept, commit the transition yourself with\n\
    tddy-tools transition --to <goal>\n\
  (no --provisional). To reject, do not commit; send the subagent back to fix its work.\n\
- When the final goal is complete, stop.\n\
\n\
--- Current goal ('{current}') instructions ---\n\
{instructions}",
            name = self.name(),
            goals = goals,
            current = current,
            instructions = self.goal_instructions(current),
        )
    }

    /// Tool/permission hints for the **agent-driven orchestrator** invoke.
    ///
    /// One long-lived session performs the work of every goal, so the default unions all goals'
    /// `allowed_tools`, adds the `Agent` tool (for spawning subagents), and runs with edit
    /// permission. Recipes may override.
    fn orchestration_hints(&self, current: &GoalId) -> GoalHints {
        let mut base = self.goal_hints(current).unwrap_or_else(|| GoalHints {
            display_name: current.to_string(),
            permission: PermissionHint::AcceptEdits,
            allowed_tools: vec![],
            default_model: None,
            agent_output: true,
            agent_cli_plan_mode: false,
            claude_nonzero_exit_ok_if_structured_response: false,
        });
        let mut tools: std::collections::BTreeSet<String> =
            base.allowed_tools.iter().cloned().collect();
        for g in self.goal_ids() {
            if let Some(h) = self.goal_hints(&g) {
                tools.extend(h.allowed_tools);
            }
        }
        tools.insert("Agent".to_string());
        base.allowed_tools = tools.into_iter().collect();
        base.permission = PermissionHint::AcceptEdits;
        base.agent_output = true;
        // The orchestrator drives many goals in one session; vendor "plan mode" (single-goal) and
        // structured-exit handling do not apply.
        base.agent_cli_plan_mode = false;
        base.claude_nonzero_exit_ok_if_structured_response = false;
        base
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId>;

    /// Like [`next_goal_for_state`](Self::next_goal_for_state) but with full changeset access,
    /// for recipes whose resume target depends on more than the bare state string alone.
    ///
    /// Defaults to ignoring `changeset` and delegating to `next_goal_for_state`. Override when a
    /// persisted state string is ambiguous on its own — e.g. a recipe consolidation where an old
    /// standalone recipe's `initial_state()` collides with a state the new unified recipe also
    /// produces, but the two must resume differently (disambiguate using changeset fields such as
    /// `stack`, which the bare state string can't carry).
    fn next_goal_for_state_with_changeset(
        &self,
        state: &WorkflowState,
        changeset: &Changeset,
    ) -> Option<GoalId> {
        let _ = changeset;
        self.next_goal_for_state(state)
    }

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
