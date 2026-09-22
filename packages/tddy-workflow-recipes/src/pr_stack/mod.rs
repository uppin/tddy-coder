//! **pr-stack** workflow: unified PR-stack planning + orchestration recipe.
//!
//! Consolidates the two-session `plan-pr-stack` + `orchestrate-pr-stack` flow into a single
//! session/recipe: `analyze-stack` → `write-stack-plan` → `write-stack-docs` → `orchestrate`.
//! `orchestrate` is a single interactive goal with no successor edge — the session pauses for
//! input after each turn and the developer drives the stack by hand through the PR-management
//! tools (there is no autonomous assess/spawn/merge/repoint cycle). The legacy CLI names
//! `"plan-pr-stack"` and `"orchestrate-pr-stack"` remain accepted as aliases that resolve to this
//! recipe (see `recipe_resolve.rs`).
//!
//! After the plan exists (state `StackPlanned`), the session can be re-entered into
//! [`WorkflowRecipe::plan_refinement_goal`] (`write-stack-plan`) for chat-driven refinement —
//! the same session, not a new one. A refinement leaves the session at `StackPlanned`, so the next
//! run regenerates the per-PR documents before returning to `orchestrate`.
//!
//! PRD: `docs/ft/coder/pr-stacking.md`. Changeset: `docs/dev/1-WIP/pr-stack-workflow-views.md`.

mod bridge;
mod hooks;

// The stack operations and the per-node documents moved to `tddy-pr-stack`; re-exported here so
// every `pr_stack::…` path keeps resolving.
pub use tddy_pr_stack::docs;
pub use tddy_pr_stack::stack_ops::*;

pub use bridge::BeginOrchestrateTask;
pub use docs::{
    node_doc_paths, validate_stack_docs, write_stack_docs, NodeDocPaths, NodeDocs, StackDocsOutput,
    NODE_CHANGESET_BASENAME, NODE_DOCS_SUBDIR, NODE_PRD_BASENAME, REQUIRED_CHANGESET_HEADINGS,
};
pub use hooks::PrStackHooks;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_core::backend::CodingBackend;
use tddy_core::changeset::Changeset;
use tddy_core::workflow::graph::{Graph, GraphBuilder};
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::GoalId;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::{GoalHints, PermissionHint, WorkflowEventSender, WorkflowRecipe};
use tddy_core::workflow::task::BackendInvokeTask;

use crate::orchestrate_pr_stack::{STACK_STATUS_JSON_BASENAME, STACK_STATUS_MD_BASENAME};
use crate::plan_pr_stack::{StackPlanOutput, PR_STACK_PLAN_MD_BASENAME, STACK_PLAN_BASENAME};
use crate::SessionArtifactManifest;

/// The workflow state a `pr-stack` session records once its stack exists: the plan is written, and
/// the next thing to run is `orchestrate`, the free-prompting operator loop.
///
/// Named here, in the crate that owns the stack, because three different writers put a session into it
/// — the planning hook that persists an agent's `stack-plan.yaml`, the legacy `plan-pr-stack` hook, and
/// `tddy-coder`'s creation-time seed (`--stack-seed-base-session`) — and every reader of a changeset's
/// state has to agree with them on the spelling. It is a persisted value: a changeset written by an
/// older build carries this exact string, so it is renamed only with a migration.
pub const STATE_STACK_PLANNED: &str = "StackPlanned";

/// The workflow state a `pr-stack` session records once every planned node owns its two documents:
/// the docs pass is done, and the next thing to run is `orchestrate`.
///
/// Sits between [`STATE_STACK_PLANNED`] and the operator loop so a *planned* stack routes to
/// `write-stack-docs` while a *documented* one routes past it. A stack seeded from an existing
/// session starts here rather than at `StackPlanned`: its one node describes work that already
/// exists, so a retroactive PRD documents a decision nobody is about to make.
///
/// Persisted, like [`STATE_STACK_PLANNED`] — renamed only with a migration.
pub const STATE_STACK_DOCS_WRITTEN: &str = "StackDocsWritten";

/// MCP tool names the orchestrator agent uses to manage the stack during the `orchestrate` goal.
pub const PR_STACK_TOOL_NAMES: &[&str] = &[
    "mcp__tddy-tools__pr_stack_status",
    "mcp__tddy-tools__pr_merge",
    "mcp__tddy-tools__pr_repoint",
    "mcp__tddy-tools__pr_close",
    "mcp__tddy-tools__pr_resolve_conflicts",
    "mcp__tddy-tools__pr_set_status",
    "mcp__tddy-tools__pr_add_planned",
    "mcp__tddy-tools__pr_spawn_child",
    "mcp__tddy-tools__pr_update_planned",
    "mcp__tddy-tools__pr_delete_planned",
    "mcp__tddy-tools__pr_set_parents",
    "mcp__tddy-tools__pr_read",
    "mcp__tddy-tools__pr_search",
    "mcp__tddy-tools__pr_comments",
    "mcp__tddy-tools__pr_adopt",
];

/// System prompt for the interactive `orchestrate` goal. Unlike the default orchestration prompt,
/// it does NOT tell the agent to self-advance a state machine — the developer drives, turn by turn.
const PR_STACK_ORCHESTRATE_PROMPT: &str = "\
You are operating a stack of pull requests together with the developer. The plan is written and \
the stack nodes exist. This is an interactive chat: respond to each of the developer's prompts and \
manage the stack on request. Do NOT loop autonomously and do NOT try to advance a state machine — \
wait for the developer's instructions each turn.\n\
\n\
You have these tools to manage the stack:\n\
- pr_stack_status — list every PR node with its live GitHub state and computed internal status \
(needs-repoint / has-conflicts / ready-to-merge / merged / up-to-date). Run this to see what needs \
action.\n\
- pr_merge — merge a node's PR into its base.\n\
- pr_repoint — repoint a node's PR base branch after an ancestor merges.\n\
- pr_close — close a PR without merging.\n\
- pr_resolve_conflicts — sync a node's branch with its base and report conflicting files; then \
resolve them in the worktree and re-run to confirm a clean tree.\n\
- pr_set_status — record a manual internal-status override with a note (e.g. blocked).\n\
- pr_add_planned — add a new planned PR node to the stack.\n\
- pr_spawn_child — start a child coding session for a planned node.\n\
- pr_update_planned — edit a node's title, description or branch_suggestion. Title and description \
are editable at any time; branch_suggestion only while the node owns no branch. Pass sync_pr to push \
the new title/body to the node's PR as well.\n\
- pr_delete_planned — remove a node from the plan, reparenting its children onto that node's \
parents. Refuses a node whose PR is open — merge or close it first. The node's branch, worktree and \
child session are left alone and reported back as unowned.\n\
- pr_set_parents — move a node in the stack: give it a whole new parent list (empty means it becomes \
a root, based off the stack bottom). Use this when the *plan* changed; use pr_repoint when only the \
PR's base branch drifted after an ancestor merged. Since a stack's order is derived from parents, \
this is also how you reorder.\n\
- pr_read — read one PR in full: title, body, state, base/head, mergeability, the latest review \
state per reviewer, and the head commit's check runs. Pass include_files for the changed-file list.\n\
- pr_search — find PRs in this repository, including ones the stack does not track, by text, state, \
author or base. A hit reports no head or base branch (GitHub's search does not return them) — follow \
up with pr_read when you need the branches.\n\
- pr_comments — read a PR's review feedback: submitted reviews, diff-anchored comment threads, and \
conversation comments. A thread's resolved/unresolved state is not available over this API, so no \
thread is reported as resolved — read the replies to judge.\n\
- pr_adopt — bring an existing PR into the stack as a node bound to its head branch, choosing which \
nodes it stacks on.\n\
\n\
When unsure what to do next, run pr_stack_status and report the state to the developer.";

/// **pr-stack** recipe: `analyze-stack` → `write-stack-plan` → `write-stack-docs` → `orchestrate`
/// (interactive loop).
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
            recipe.clone(),
            backend.clone(),
        ));
        // Documents are authored in their own goal rather than folded into the plan submit: the
        // plan is cheap and refined constantly through chat, the documents are expensive and
        // mostly stable, and only the plan can be validated structurally by the host.
        let write_docs = Arc::new(BackendInvokeTask::from_recipe(
            "write-stack-docs",
            GoalId::new("write-stack-docs"),
            recipe.clone(),
            backend.clone(),
        ));
        // `orchestrate` is a single interactive goal with NO outgoing edge: `FlowRunner` finds no
        // successor after each backend turn and pauses as `WaitingForInput`, keeping the session
        // `Running` for a multi-turn operator chat. The developer drives the stack by hand through
        // the PR-management tools — there is no autonomous assess/spawn/merge/repoint cycle.
        let orchestrate = Arc::new(BackendInvokeTask::from_recipe(
            "orchestrate",
            GoalId::new("orchestrate"),
            recipe,
            backend,
        ));

        GraphBuilder::new("pr_stack")
            .add_task(analyze)
            .add_task(write_plan)
            .add_task(write_docs)
            .add_task(orchestrate)
            .add_edge("analyze-stack", "write-stack-plan")
            .add_edge("write-stack-plan", "write-stack-docs")
            .add_edge("write-stack-docs", "orchestrate")
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
            "write-stack-docs" => Some(GoalHints {
                display_name: "Write stack docs".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: true,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: true,
            }),
            "orchestrate" => Some(GoalHints {
                display_name: "Orchestrate stack".to_string(),
                // The agent edits files when resolving conflicts, so it needs write access.
                permission: PermissionHint::AcceptEdits,
                allowed_tools: PR_STACK_TOOL_NAMES
                    .iter()
                    .map(|s| s.to_string())
                    .chain(std::iter::once("Agent".to_string()))
                    .collect(),
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
            GoalId::new("analyze-stack"),
            GoalId::new("write-stack-plan"),
            GoalId::new("write-stack-docs"),
            GoalId::new("orchestrate"),
        ]
    }

    fn submit_key(&self, goal_id: &GoalId) -> GoalId {
        goal_id.clone()
    }

    fn next_goal_for_state(&self, state: &WorkflowState) -> Option<GoalId> {
        match state.as_str() {
            "Init" | "AnalyzeStack" => Some(GoalId::new("analyze-stack")),
            "WriteStackPlan" => Some(GoalId::new("write-stack-plan")),
            // A planned stack documents itself before the operator starts driving it; a documented
            // one — and every legacy mid-flight state — drops straight into the loop.
            STATE_STACK_PLANNED => Some(GoalId::new("write-stack-docs")),
            "done" | "Done" | "failed" | "Failed" => None,
            // Any planned/mid-flight state drops into the interactive orchestrate loop.
            _ => Some(GoalId::new("orchestrate")),
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
                return Some(GoalId::new("orchestrate"));
            }
        }
        self.next_goal_for_state(state)
    }

    fn orchestration_system_prompt(&self, current: &GoalId) -> String {
        match current.as_str() {
            "orchestrate" => PR_STACK_ORCHESTRATE_PROMPT.to_string(),
            other => format!(
                "You are working the '{other}' goal of the pr-stack workflow. Study the feature and \
                 write the PR-stack plan (stack-plan.yaml) via `tddy-tools submit`. Each planned PR \
                 must be self-contained — the API/schema change, its implementation, and its tests \
                 in one node; never split a stack by layer (schema then behavior), and split an \
                 oversized slice by capability instead. Once the plan is written the session moves \
                 on to the interactive orchestrate phase, where you and the developer manage the \
                 stack together."
            ),
        }
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
        matches!(goal_id.as_str(), "write-stack-plan" | "write-stack-docs")
    }
}

impl SessionArtifactManifest for PrStackRecipe {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)] {
        &[
            ("stack_plan", STACK_PLAN_BASENAME),
            ("stack_plan_md", PR_STACK_PLAN_MD_BASENAME),
            ("stack_status_md", STACK_STATUS_MD_BASENAME),
            ("stack_status_json", STACK_STATUS_JSON_BASENAME),
            ("exploration", crate::writer::EXPLORATION_BASENAME),
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
        a.insert(
            "exploration".to_string(),
            crate::writer::EXPLORATION_BASENAME.to_string(),
        );
        a
    }

    fn primary_document_basename(&self) -> Option<String> {
        None
    }

    fn artifact_doc_descriptions(&self) -> BTreeMap<&'static str, &'static str> {
        let mut d = BTreeMap::new();
        d.insert(
            "exploration",
            "Code-discovery exploration notes gathered before planning.",
        );
        d.insert("stack_plan", "The PR stack plan (machine-readable YAML).");
        d.insert(
            "stack_plan_md",
            "Human-readable rendering of the PR stack plan.",
        );
        d.insert(
            "stack_status_md",
            "Human-readable snapshot of each PR node's live status.",
        );
        d.insert(
            "stack_status_json",
            "Machine-readable snapshot of each PR node's live status.",
        );
        d
    }
}

/// Re-seed the orchestrator's `Changeset.stack` from a refined [`StackPlanOutput`], but only
/// while no node has been materialized yet.
///
/// Unlike [`crate::orchestrate_pr_stack::bridge::seed_orchestrator_stack_from_plan`] (which only
/// seeds an *empty* stack), this overwrites `version` + `nodes` wholesale — the refine-after-plan
/// chat loop calls this every time the agent re-emits `stack-plan.yaml`. Once any node owns a
/// `branch` or a `session_id`, the refinement is refused: the branch is real work the stack is
/// built on, and it outlives the child session that created it.
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
        if stack
            .nodes
            .iter()
            .any(|n| n.branch.is_some() || n.session_id.is_some())
        {
            return Err(
                "reseed_stack_from_plan_if_unspawned: refusing to overwrite a stack whose nodes already own a branch or a child session"
                    .to_string(),
            );
        }
    }

    // A re-seed replaces the plan, so it replaces the reading order with it: every node is numbered
    // from the new plan's array order (see `planned_prs_into_stack_nodes`) rather than inheriting a
    // position from the stack being discarded. Nothing is left unnumbered for
    // `assign_missing_display_order` to pick up.
    let nodes = crate::plan_pr_stack::planned_prs_into_stack_nodes(&plan.prs);
    tddy_core::changeset::update_stack_atomic(session_dir, |stack| {
        stack.version = plan.version;
        stack.nodes = nodes;
    })
    .map_err(|e| format!("reseed_stack_from_plan_if_unspawned: failed to write stack: {e}"))
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
    // Artifact manifest (context docs)
    // -----------------------------------------------------------------------

    #[test]
    fn known_artifacts_include_exploration_so_it_is_surfaced_as_context() {
        // Given — the unified pr-stack recipe's artifact manifest
        let recipe = PrStackRecipe;

        // When
        let artifacts = recipe.known_artifacts();

        // Then — exploration.md is a known artifact, so it can be listed as a context doc and
        // injected into the orchestrate goal's context-reminder header (like tdd/tdd-small/bugfix).
        assert!(
            artifacts.contains(&("exploration", "exploration.md")),
            "known_artifacts must include the exploration doc; got: {artifacts:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Resume / next_goal_for_state
    // -----------------------------------------------------------------------

    #[test]
    fn resuming_a_planned_stack_continues_into_the_docs_pass() {
        // Given — the plan exists and the session was closed/reopened
        let recipe = PrStackRecipe;
        let state = WorkflowState::new("StackPlanned");

        // When
        let next = recipe.next_goal_for_state(&state);

        // Then — a planned stack documents itself before the operator drives it
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("write-stack-docs".to_string())
        );
    }

    #[rstest]
    #[case::stack_documented("StackDocsWritten")]
    #[case::legacy_assess("assess")]
    #[case::legacy_wait("wait")]
    fn every_non_terminal_state_resumes_into_the_orchestrate_loop(#[case] state_name: &str) {
        // Given — including legacy persisted loop-state names from the removed auto-loop
        let recipe = PrStackRecipe;
        let state = WorkflowState::new(state_name);

        // When
        let next = recipe.next_goal_for_state(&state);

        // Then
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("orchestrate".to_string()),
            "state {state_name} should resume at orchestrate"
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
    fn a_legacy_orchestrator_session_stuck_at_init_with_a_populated_stack_resumes_into_orchestrate()
    {
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
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Changeset::default()
        };

        // When
        let next = recipe.next_goal_for_state_with_changeset(&state, &changeset);

        // Then — continues orchestrating, does not restart planning
        assert_eq!(
            next.map(|g| g.as_str().to_string()),
            Some("orchestrate".to_string())
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
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Changeset::default()
        };

        // When
        let goal = tddy_core::changeset::start_goal_for_session_continue(&recipe, &changeset);

        // Then
        assert_eq!(goal.as_str(), "orchestrate");
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
    // build_graph — plan phase flows into the terminal interactive orchestrate goal
    // -----------------------------------------------------------------------

    #[test]
    fn graph_flows_plan_phase_into_a_terminal_orchestrate_goal() {
        // Given
        let backend = Arc::new(StubBackend::new());
        let recipe = PrStackRecipe;
        let graph = recipe.build_graph(backend);
        let ctx = Context::new();

        // When / Then — one session walks analyze -> write-plan -> orchestrate, then pauses
        assert_eq!(
            graph.next_task_id("analyze-stack", &ctx),
            Some("write-stack-plan".to_string()),
            "edge analyze-stack -> write-stack-plan"
        );
        assert_eq!(
            graph.next_task_id("write-stack-plan", &ctx),
            Some("write-stack-docs".to_string()),
            "edge write-stack-plan -> write-stack-docs"
        );
        assert_eq!(
            graph.next_task_id("write-stack-docs", &ctx),
            Some("orchestrate".to_string()),
            "edge write-stack-docs -> orchestrate"
        );
        assert_eq!(
            graph.next_task_id("orchestrate", &ctx),
            None,
            "orchestrate is terminal (no successor) so FlowRunner pauses for input"
        );
    }

    #[test]
    fn graph_has_no_autonomous_loop_tasks() {
        // Given
        let backend = Arc::new(StubBackend::new());
        let graph = PrStackRecipe.build_graph(backend);

        // Then — the removed auto-loop tasks are gone
        for removed in ["begin-orchestrate", "assess", "spawn", "merge", "repoint"] {
            assert!(
                graph.get_task(removed).is_none(),
                "auto-loop task '{removed}' must be removed from the pr-stack graph"
            );
        }
    }

    // -----------------------------------------------------------------------
    // reseed_stack_from_plan_if_unspawned
    // -----------------------------------------------------------------------

    fn a_two_node_plan() -> StackPlanOutput {
        use crate::plan_pr_stack::PlannedPr;
        StackPlanOutput {
            version: 1,
            exploration: None,
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
                    internal_status: None,
                    display_order: None,
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
                    internal_status: None,
                    display_order: None,
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
    fn reseeding_refuses_to_overwrite_a_stack_once_a_node_owns_a_branch() {
        // Given — node n1 owns a real branch; no session is attached to it (it was closed)
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
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Changeset::default()
        };
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — a chat refinement tries to reshape the plan
        let result = reseed_stack_from_plan_if_unspawned(dir, &a_two_node_plan());

        // Then — refused: the branch is real work, whether or not a session still points at it
        assert!(
            result.is_err(),
            "expected Err once a node owns a materialized branch"
        );
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(
            loaded.node("n1").unwrap().branch.as_deref(),
            Some("feature/token-store")
        );
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
                    internal_status: None,
                    display_order: None,
                }],
            }),
            ..Changeset::default()
        };
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the agent's refined plan has a cycle (n1 depends on n2, n2 depends on n1)
        let cyclic_plan = StackPlanOutput {
            version: 2,
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
    // tool surface
    // -----------------------------------------------------------------------

    #[test]
    fn the_orchestrate_goal_allows_every_pr_management_tool_its_prompt_documents() {
        // Given — the prompt the agent is handed for the interactive goal, and the allowlist the same
        // goal is started with
        let prompt = PR_STACK_ORCHESTRATE_PROMPT;
        let allowlist = PR_STACK_TOOL_NAMES;

        // When — each is reduced to the set of PR-management tool names it names
        let documented: Vec<String> = prompt
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .filter_map(|line| line.split_whitespace().next())
            .filter(|name| name.starts_with("pr_"))
            .map(str::to_string)
            .collect();
        let allowed: Vec<String> = allowlist
            .iter()
            .filter_map(|name| name.strip_prefix("mcp__tddy-tools__"))
            .map(str::to_string)
            .collect();

        // Then — a tool described to the agent but not allowed is a tool it will try and fail to
        // call; a tool allowed but never described is one it will never think to use
        assert_eq!(
            documented, allowed,
            "the orchestrate prompt and the tool allowlist must describe the same set"
        );
    }
}
