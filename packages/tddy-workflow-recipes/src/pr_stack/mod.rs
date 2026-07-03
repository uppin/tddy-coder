//! **pr-stack** workflow: unified PR-stack planning + orchestration recipe.
//!
//! Consolidates the two-session `plan-pr-stack` + `orchestrate-pr-stack` flow into a single
//! session/recipe: `analyze-stack` → `write-stack-plan` → `begin-orchestrate` → `assess` loop
//! (`assess` → `{spawn|merge|end}`, `spawn` → `assess`, `merge` → `repoint`, `repoint` → `assess`,
//! matching [`crate::orchestrate_pr_stack::OrchestratePrStackRecipe::build_graph`] exactly for the
//! loop portion). The legacy CLI names `"plan-pr-stack"` and `"orchestrate-pr-stack"` remain
//! accepted as aliases that resolve to this recipe (see `recipe_resolve.rs`).
//!
//! After the plan exists (state `StackPlanned`), the session can be re-entered into
//! [`WorkflowRecipe::plan_refinement_goal`] (`write-stack-plan`) for chat-driven refinement —
//! the same session, not a new one — before continuing into `assess` on resume.
//!
//! PRD: `docs/ft/coder/pr-stacking.md`. Changeset: `docs/dev/1-WIP/pr-stack-workflow-views.md`.

mod bridge;
mod hooks;

pub use bridge::BeginOrchestrateTask;
pub use hooks::PrStackHooks;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::{CodingBackend, GoalHints, GoalId, PermissionHint};
use tddy_core::changeset::{Changeset, StackNode};
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::{BackendInvokeTask, EndTask};

use crate::orchestrate_pr_stack::{
    AssessTask, MergeTask, RepointTask, SpawnTask, STACK_STATUS_JSON_BASENAME,
    STACK_STATUS_MD_BASENAME,
};
use crate::plan_pr_stack::{StackPlanOutput, PR_STACK_PLAN_MD_BASENAME, STACK_PLAN_BASENAME};
use crate::SessionArtifactManifest;

/// **pr-stack** recipe: `analyze-stack` → `write-stack-plan` → `begin-orchestrate` → `assess` loop.
#[derive(Clone, Copy, Default, Debug)]
pub struct PrStackRecipe;

impl WorkflowRecipe for PrStackRecipe {
    fn name(&self) -> &str {
        "pr-stack"
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
        let begin_orchestrate = Arc::new(BeginOrchestrateTask::new());
        let assess = Arc::new(AssessTask::new());
        let spawn = Arc::new(SpawnTask::new());
        let merge = Arc::new(MergeTask::new());
        let repoint = Arc::new(RepointTask::new());
        let end = Arc::new(EndTask::new("end"));

        GraphBuilder::new("pr_stack")
            .add_task(analyze)
            .add_task(write_plan)
            .add_task(begin_orchestrate)
            .add_task(assess)
            .add_task(spawn)
            .add_task(merge)
            .add_task(repoint)
            .add_task(end)
            .add_edge("analyze-stack", "write-stack-plan")
            .add_edge("write-stack-plan", "begin-orchestrate")
            .add_edge("begin-orchestrate", "assess")
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
        Arc::new(PrStackHooks::new(event_tx))
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
        vec![
            GoalId::new("analyze-stack"),
            GoalId::new("write-stack-plan"),
            GoalId::new("assess"),
        ]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "AnalyzeStack" => Some(GoalId::new("analyze-stack")),
            "WriteStackPlan" => Some(GoalId::new("write-stack-plan")),
            "done" | "Done" | "failed" | "Failed" => None,
            _ => Some(GoalId::new("assess")),
        }
    }

    fn next_goal_for_state_with_changeset(
        &self,
        state: &WorkflowState,
        changeset: &Changeset,
    ) -> Option<GoalId> {
        // "Init" is ambiguous: it's the bootstrap state AND (via the legacy
        // "orchestrate-pr-stack" alias) the initial_state a pre-consolidation orchestrator
        // session may still be sitting at, since that recipe's own state never advanced past
        // "Init" during healthy operation. Disambiguate using the changeset: a populated stack
        // means orchestration is already under way, so resume into the loop instead of
        // restarting analysis.
        if state.as_str() == "Init" {
            let stack_in_progress = changeset
                .stack
                .as_ref()
                .is_some_and(|s| !s.nodes.is_empty());
            if stack_in_progress {
                return Some(GoalId::new("assess"));
            }
        }
        self.next_goal_for_state(state)
    }

    fn status_for_state(&self, state: &WorkflowState) -> &'static str {
        match state.as_str() {
            "failed" | "Failed" => "Failed",
            "done" | "Done" => "Completed",
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
            log::info!("[pr-stack:{}] output:\n{}", goal_id.as_str(), o);
        }
        Ok(())
    }

    fn goal_requires_tddy_tools_submit(&self, goal_id: &GoalId) -> bool {
        goal_id.as_str() == "write-stack-plan"
    }
}

impl SessionArtifactManifest for PrStackRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("stack_plan", STACK_PLAN_BASENAME),
            ("stack_plan_md", PR_STACK_PLAN_MD_BASENAME),
            ("stack_status_md", STACK_STATUS_MD_BASENAME),
            ("stack_status_json", STACK_STATUS_JSON_BASENAME),
        ]
    }

    fn default_artifacts(&self) -> BTreeMap<String, String> {
        let mut a = BTreeMap::new();
        a.insert("stack_plan".to_string(), STACK_PLAN_BASENAME.to_string());
        a.insert(
            "stack_plan_md".to_string(),
            PR_STACK_PLAN_MD_BASENAME.to_string(),
        );
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

/// Re-seed the orchestrator's `Changeset.stack` from a refined [`StackPlanOutput`], but only
/// while no node has been materialized into a child session yet.
///
/// Unlike [`crate::orchestrate_pr_stack::bridge::seed_orchestrator_stack_from_plan`] (which only
/// seeds an *empty* stack), this overwrites `version` + `nodes` wholesale — the refine-after-plan
/// chat loop calls this every time the agent re-emits `stack-plan.yaml`. Once any node has a
/// `session_id`, the refinement is refused so an in-progress child session is never orphaned.
///
/// Validates the incoming plan (unique node ids, no dangling parents, no cycle) before touching
/// disk — an invalid refinement leaves the previously-persisted stack untouched.
pub fn reseed_stack_from_plan_if_unspawned(
    session_dir: &Path,
    plan: &StackPlanOutput,
) -> Result<(), String> {
    crate::plan_pr_stack::validate_stack_plan(plan)
        .map_err(|e| format!("reseed_stack_from_plan_if_unspawned: {e}"))?;

    let changeset = tddy_core::changeset::read_changeset(session_dir).map_err(|e| {
        format!("reseed_stack_from_plan_if_unspawned: failed to read changeset: {e}")
    })?;
    if let Some(stack) = changeset.stack.as_ref() {
        if stack.nodes.iter().any(|n| n.session_id.is_some()) {
            return Err(
                "reseed_stack_from_plan_if_unspawned: refusing to overwrite a stack that already has a spawned child session"
                    .to_string(),
            );
        }
    }

    let nodes = crate::plan_pr_stack::planned_prs_into_stack_nodes(&plan.prs);
    tddy_core::changeset::update_stack_atomic(session_dir, |stack| {
        stack.version = plan.version;
        stack.nodes = nodes;
    })
    .map_err(|e| format!("reseed_stack_from_plan_if_unspawned: failed to write stack: {e}"))
}

/// Input for [`add_planned_pr_node`]. A struct rather than positional params since several
/// fields share the same `Option<String>` shape — grouping them removes the transposition risk.
pub struct AddPlannedPrInput {
    pub title: String,
    pub description: String,
    pub branch_suggestion: Option<String>,
    pub parents: Vec<String>,
    /// Accepted for symmetry with [`crate::plan_pr_stack::PlannedPr`] but currently unused: like
    /// [`crate::plan_pr_stack::planned_prs_into_stack_nodes`], `StackNode` has no `child_recipe`
    /// field to carry it — the web client defaults to `"tdd"` at start-session time regardless
    /// (see `PrStackScreen.tsx`'s `handleStartSession`).
    pub child_recipe: Option<String>,
}

/// Append one manually-created planned PR to an orchestrator session's persisted stack,
/// choosing its ancestors (parent node ids) from the already-planned nodes.
///
/// Unlike [`reseed_stack_from_plan_if_unspawned`] (agent-driven, replaces the whole plan
/// wholesale and refuses once any node has spawned), this appends a single node and never
/// touches existing nodes — safe to call regardless of how many nodes have already spawned
/// child sessions.
///
/// The new node's `node_id` is always server-assigned (see [`next_free_node_id`]) — callers
/// never supply one. Rejects (without writing) a `parents` entry that doesn't resolve to an
/// existing node, or an append that would introduce a cycle.
///
/// PRD: `docs/ft/coder/pr-stacking.md` § Manually adding a planned PR.
pub fn add_planned_pr_node(
    session_dir: &Path,
    input: AddPlannedPrInput,
) -> Result<StackNode, String> {
    let changeset = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("add_planned_pr_node: failed to read changeset: {e}"))?;
    let existing = changeset.stack.unwrap_or_default();

    for parent in &input.parents {
        if !existing.nodes.iter().any(|n| &n.node_id == parent) {
            return Err(format!("dangling parent ref: {parent}"));
        }
    }

    let node_id = next_free_node_id(&existing);
    let new_node = StackNode {
        node_id,
        title: input.title,
        description: input.description,
        branch_suggestion: input.branch_suggestion,
        branch: None,
        session_id: None,
        parents: input.parents,
        pr_status: None,
        child_state: None,
    };

    // Defense-in-depth cycle check: parents are restricted to pre-existing node ids above, so an
    // append alone can never actually cycle, but this keeps the same guard `validate_stack_plan`
    // applies to a whole plan, cheaply, rather than special-casing the append path as exempt.
    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    let candidate_stack = tddy_core::changeset::Stack {
        version: existing.version,
        nodes: candidate_nodes,
    };
    candidate_stack
        .topo_order()
        .map_err(|e| format!("cycle detected: {e}"))?;

    tddy_core::changeset::update_stack_atomic(session_dir, |stack| {
        stack.nodes.push(new_node.clone());
    })
    .map_err(|e| format!("add_planned_pr_node: failed to write stack: {e}"))?;

    Ok(new_node)
}

/// Next free `"n<N>"` node id for a stack: one past the highest existing numeric suffix among
/// ids matching `n<digits>` (non-matching ids are ignored for this purpose), or `"n1"` for an
/// empty/all-non-matching stack. Uses the max, not the count, so a stack with a gap (e.g. `"n1"`,
/// `"n5"`) still assigns `"n6"` rather than colliding.
fn next_free_node_id(stack: &tddy_core::changeset::Stack) -> String {
    let max = stack
        .nodes
        .iter()
        .filter_map(|n| n.node_id.strip_prefix('n')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("n{}", max + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use tddy_core::backend::StubBackend;
    use tddy_core::changeset::{read_changeset, GithubPrStatus, Stack, StackNode};
    use tddy_core::workflow::context::Context;

    // -----------------------------------------------------------------------
    // Recipe identity
    // -----------------------------------------------------------------------

    #[test]
    fn recipe_name_is_pr_stack() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let name = recipe.name();

        // Then
        assert_eq!(name, "pr-stack");
    }

    #[test]
    fn initial_state_is_analyze_stack() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let state = recipe.initial_state();

        // Then
        assert_eq!(state.as_str(), "AnalyzeStack");
    }

    #[test]
    fn start_goal_is_analyze_stack() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let goal = recipe.start_goal();

        // Then
        assert_eq!(goal.as_str(), "analyze-stack");
    }

    #[test]
    fn plan_refinement_goal_is_write_stack_plan_so_chat_can_refine_an_existing_plan() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let goal = recipe.plan_refinement_goal();

        // Then
        assert_eq!(goal.as_str(), "write-stack-plan");
    }

    // -----------------------------------------------------------------------
    // Resume / next_goal_for_state
    // -----------------------------------------------------------------------

    #[test]
    fn resuming_a_planned_stack_continues_into_the_assess_loop() {
        // Given — the plan exists and the session was closed/reopened
        let recipe = PrStackRecipe;
        let state = WorkflowState::new("StackPlanned");

        // When
        let next = recipe.next_goal_for_state(&state);

        // Then
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("assess".to_string())
        );
    }

    #[rstest]
    #[case::assess("assess")]
    #[case::spawn("spawn")]
    #[case::merge("merge")]
    #[case::repoint("repoint")]
    #[case::wait("wait")]
    fn every_non_terminal_orchestrate_loop_state_resumes_at_assess(#[case] state_name: &str) {
        // Given
        let recipe = PrStackRecipe;
        let state = WorkflowState::new(state_name);

        // When
        let next = recipe.next_goal_for_state(&state);

        // Then
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("assess".to_string()),
            "state {state_name} should resume at assess"
        );
    }

    #[rstest]
    #[case::done("done")]
    #[case::failed("failed")]
    fn terminal_orchestrate_states_have_no_next_goal(#[case] state_name: &str) {
        // Given
        let recipe = PrStackRecipe;
        let state = WorkflowState::new(state_name);

        // When
        let next = recipe.next_goal_for_state(&state);

        // Then
        assert_eq!(next, None, "terminal state {state_name} must not resume");
    }

    // -----------------------------------------------------------------------
    // Legacy resume: a pre-consolidation "orchestrate-pr-stack" session's own state never
    // advanced past "Init" during healthy operation, so "Init" is ambiguous between "brand new
    // pr-stack session" and "orchestration already under way" — disambiguate via the changeset.
    // -----------------------------------------------------------------------

    #[test]
    fn a_legacy_orchestrator_session_stuck_at_init_with_a_populated_stack_resumes_into_assess() {
        // Given — an old orchestrate-pr-stack session whose state never left "Init" but whose
        // stack already has nodes (orchestration is mid-flight)
        let recipe = PrStackRecipe;
        let state = WorkflowState::new("Init");
        let changeset = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Add token store".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                }],
            }),
            ..Changeset::default()
        };

        // When
        let next = recipe.next_goal_for_state_with_changeset(&state, &changeset);

        // Then — continues orchestrating, does not restart planning
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("assess".to_string())
        );
    }

    #[test]
    fn a_brand_new_session_at_init_with_no_stack_yet_resumes_into_analyze_stack() {
        // Given — a genuinely fresh session (or one whose plan hasn't been written yet)
        let recipe = PrStackRecipe;
        let state = WorkflowState::new("Init");
        let changeset = Changeset::default();

        // When
        let next = recipe.next_goal_for_state_with_changeset(&state, &changeset);

        // Then
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("analyze-stack".to_string())
        );
    }

    #[test]
    fn resuming_a_legacy_orchestrator_session_end_to_end_via_start_goal_for_session_continue() {
        // Given — start_goal_for_session_continue is the real call site used on session resume;
        // it has full changeset access and must route through next_goal_for_state_with_changeset.
        let recipe = PrStackRecipe;
        let changeset = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Add token store".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                }],
            }),
            ..Changeset::default()
        };

        // When
        let goal = tddy_core::changeset::start_goal_for_session_continue(&recipe, &changeset);

        // Then
        assert_eq!(goal.as_str(), "assess");
    }

    // -----------------------------------------------------------------------
    // status_for_state — regression guard: StackPlanned is Active, not Completed
    // -----------------------------------------------------------------------

    #[test]
    fn stack_planned_status_is_active_because_the_session_continues_into_orchestration() {
        // Given — unlike the old plan-pr-stack recipe, the unified recipe does not stop at the
        // plan: the session goes on to orchestrate the same stack, so a dashboard must not treat
        // "plan written" as "session done".
        let recipe = PrStackRecipe;
        let state = WorkflowState::new("StackPlanned");

        // When
        let status = recipe.status_for_state(&state);

        // Then
        assert_eq!(status, "Active");
    }

    #[test]
    fn done_status_is_completed() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let status = recipe.status_for_state(&WorkflowState::new("done"));

        // Then
        assert_eq!(status, "Completed");
    }

    #[test]
    fn failed_status_is_failed() {
        // Given
        let recipe = PrStackRecipe;

        // When
        let status = recipe.status_for_state(&WorkflowState::new("failed"));

        // Then
        assert_eq!(status, "Failed");
    }

    // -----------------------------------------------------------------------
    // build_graph — bridge edge from the plan phase into the orchestrate loop
    // -----------------------------------------------------------------------

    #[test]
    fn graph_bridges_the_plan_phase_into_the_orchestrate_loop_in_one_session() {
        // Given
        let backend = Arc::new(StubBackend::new());
        let recipe = PrStackRecipe;
        let graph = recipe.build_graph(backend);
        let ctx = Context::new();

        // When / Then — one session walks plan -> bridge -> loop without a second recipe
        assert_eq!(
            graph.next_task_id("analyze-stack", &ctx),
            Some("write-stack-plan".to_string()),
            "edge analyze-stack -> write-stack-plan"
        );
        assert_eq!(
            graph.next_task_id("write-stack-plan", &ctx),
            Some("begin-orchestrate".to_string()),
            "edge write-stack-plan -> begin-orchestrate (the new bridge)"
        );
        assert_eq!(
            graph.next_task_id("begin-orchestrate", &ctx),
            Some("assess".to_string()),
            "edge begin-orchestrate -> assess"
        );
        assert_eq!(
            graph.next_task_id("spawn", &ctx),
            Some("assess".to_string()),
            "edge spawn -> assess (loop back, same as orchestrate-pr-stack)"
        );
        assert_eq!(
            graph.next_task_id("merge", &ctx),
            Some("repoint".to_string()),
            "edge merge -> repoint (same as orchestrate-pr-stack)"
        );
        assert_eq!(
            graph.next_task_id("repoint", &ctx),
            Some("assess".to_string()),
            "edge repoint -> assess (loop back, same as orchestrate-pr-stack)"
        );
    }

    // -----------------------------------------------------------------------
    // reseed_stack_from_plan_if_unspawned
    // -----------------------------------------------------------------------

    fn a_two_node_plan() -> StackPlanOutput {
        use crate::plan_pr_stack::PlannedPr;
        StackPlanOutput {
            version: 1,
            prs: vec![
                PlannedPr {
                    node_id: "n1".to_string(),
                    title: "Add token store".to_string(),
                    description: String::new(),
                    branch_suggestion: Some("feature/auth/token-store".to_string()),
                    parents: vec![],
                    child_recipe: None,
                },
                PlannedPr {
                    node_id: "n2".to_string(),
                    title: "Add auth middleware".to_string(),
                    description: String::new(),
                    branch_suggestion: Some("feature/auth/middleware".to_string()),
                    parents: vec!["n1".to_string()],
                    child_recipe: None,
                },
            ],
        }
    }

    #[test]
    fn reseeding_an_unspawned_stack_overwrites_it_with_the_refined_plan() {
        // Given — a session whose stack has not spawned any child yet
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Old title before refinement".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                }],
            }),
            ..Changeset::default()
        };
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the operator chats a refinement that reshapes the plan into two nodes
        let result = reseed_stack_from_plan_if_unspawned(dir, &a_two_node_plan());

        // Then
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 2);
        let n1 = loaded.node("n1").unwrap();
        assert_eq!(n1.title, "Add token store");
    }

    #[test]
    fn reseeding_refuses_to_overwrite_a_stack_once_a_node_has_a_spawned_child_session() {
        // Given — node n1 already has a materialized child session
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Add token store".to_string(),
                    description: String::new(),
                    branch_suggestion: Some("feature/token-store".to_string()),
                    branch: Some("feature/token-store".to_string()),
                    session_id: Some("child-session-1".to_string()),
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                }],
            }),
            ..Changeset::default()
        };
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — a chat refinement tries to reshape the plan after n1 was already spawned
        let result = reseed_stack_from_plan_if_unspawned(dir, &a_two_node_plan());

        // Then — refused, and the spawned node's session link survives untouched
        assert!(
            result.is_err(),
            "expected Err once a node has a spawned child session"
        );
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        let n1 = loaded.node("n1").unwrap();
        assert_eq!(n1.session_id.as_deref(), Some("child-session-1"));
    }

    #[test]
    fn reseeding_rejects_a_refinement_that_introduces_a_cycle_and_preserves_the_previous_stack() {
        use crate::plan_pr_stack::PlannedPr;

        // Given — a valid, previously-persisted single-node stack
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = Changeset {
            stack: Some(Stack {
                version: 1,
                nodes: vec![StackNode {
                    node_id: "n1".to_string(),
                    title: "Original node".to_string(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec![],
                    pr_status: Some(GithubPrStatus {
                        phase: "planned".to_string(),
                        url: None,
                        error: None,
                    }),
                    child_state: None,
                }],
            }),
            ..Changeset::default()
        };
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the agent's refined plan has a cycle (n1 depends on n2, n2 depends on n1)
        let cyclic_plan = StackPlanOutput {
            version: 2,
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
        let result = reseed_stack_from_plan_if_unspawned(dir, &cyclic_plan);

        // Then — rejected, and the previous valid stack is still on disk
        assert!(result.is_err(), "expected Err for a cyclic refinement");
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(
            loaded.nodes.len(),
            1,
            "previous stack must survive untouched"
        );
        assert_eq!(loaded.node("n1").unwrap().title, "Original node");
    }

    // -----------------------------------------------------------------------
    // add_planned_pr_node
    // -----------------------------------------------------------------------

    fn a_changeset_with_stack(nodes: Vec<StackNode>) -> Changeset {
        Changeset {
            stack: Some(Stack { version: 1, nodes }),
            ..Changeset::default()
        }
    }

    fn a_node(node_id: &str, title: &str, parents: Vec<&str>) -> StackNode {
        StackNode {
            node_id: node_id.to_string(),
            title: title.to_string(),
            description: String::new(),
            branch_suggestion: None,
            branch: None,
            session_id: None,
            parents: parents.into_iter().map(str::to_string).collect(),
            pr_status: None,
            child_state: None,
        }
    }

    #[test]
    fn appending_a_root_planned_pr_to_an_empty_stack_assigns_n1_and_persists_it() {
        // Given — a session with no stack yet
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();

        // When
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token store".to_string(),
                description: "Persists refresh tokens.".to_string(),
                branch_suggestion: Some("feature/token-store".to_string()),
                parents: vec![],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let node = result.unwrap();
        assert_eq!(node.node_id, "n1");
        assert_eq!(node.title, "Add token store");
        assert_eq!(node.description, "Persists refresh tokens.");
        assert_eq!(
            node.branch_suggestion.as_deref(),
            Some("feature/token-store")
        );
        assert_eq!(node.parents, Vec::<String>::new());

        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.node("n1").unwrap().title, "Add token store");
    }

    #[test]
    fn appending_a_node_with_valid_parents_persists_them_and_assigns_the_next_free_id() {
        // Given — a stack with two existing nodes, n1 and n2
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n2", "Add auth middleware", vec!["n1"]),
        ]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the new node depends on both existing nodes
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token refresh endpoint".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n1".to_string(), "n2".to_string()],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let node = result.unwrap();
        assert_eq!(node.node_id, "n3");
        assert_eq!(node.parents, vec!["n1".to_string(), "n2".to_string()]);

        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 3);
        assert_eq!(
            loaded.node("n3").unwrap().parents,
            vec!["n1".to_string(), "n2".to_string()]
        );
    }

    #[test]
    fn a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — a stack with a single node, n1
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![a_node("n1", "Add token store", vec![])]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the requested ancestor "n99" does not exist
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add auth middleware".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n99".to_string()],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_err(), "expected Err for a dangling parent ref");
        assert!(result.unwrap_err().contains("n99"));
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 1, "stack on disk must be unchanged");
    }

    #[test]
    fn the_new_node_always_stays_planned_with_no_session_id_or_pr_status() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();

        // When
        let node = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token store".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec![],
                child_recipe: None,
            },
        )
        .unwrap();

        // Then
        assert_eq!(node.session_id, None);
        assert_eq!(node.pr_status, None);
        assert_eq!(node.branch, None);
        assert_eq!(node.child_state, None);
    }

    #[test]
    fn node_id_assignment_picks_up_after_a_non_contiguous_max() {
        // Given — a stack whose highest existing node id is "n5", not the node count (2)
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n5", "Add auth middleware", vec![]),
        ]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When
        let node = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token refresh endpoint".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec![],
                child_recipe: None,
            },
        )
        .unwrap();

        // Then — next id is one past the max ("n6"), not one past the count ("n3")
        assert_eq!(node.node_id, "n6");
    }
}
