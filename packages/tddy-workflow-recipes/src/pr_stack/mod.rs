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
pub mod docs;
mod hooks;

pub use bridge::BeginOrchestrateTask;
pub use docs::{
    node_doc_paths, validate_stack_docs, write_stack_docs, NodeDocPaths, NodeDocs, StackDocsOutput,
    NODE_CHANGESET_BASENAME, NODE_DOCS_SUBDIR, NODE_PRD_BASENAME, REQUIRED_CHANGESET_HEADINGS,
};
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

/// Number every node that carries no [`StackNode::display_order`], appending them after the highest
/// position already recorded, in topological order.
///
/// Idempotent: a stack whose nodes are all numbered comes out unchanged, so every mutator can call
/// it unconditionally as its first act. That is what makes a stack written before the field existed
/// self-healing — the *next* write, about whatever it is about, records a total reading order, and
/// [`tddy_core::changeset::Stack::display_order`]'s topological fallback stops applying to it.
///
/// Topological order, not `nodes` array order: the array has never been ordered by anything, so
/// numbering from it would freeze an arbitrary sequence into the operator's view. Existing numbers
/// are never rewritten — a gap left by a delete is harmless in a sort key, and closing it would move
/// rows nobody touched.
pub(crate) fn assign_missing_display_order(stack: &mut tddy_core::changeset::Stack) {
    let mut next = next_display_order(stack);
    for node_id in numbering_order(stack) {
        let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) else {
            continue;
        };
        if node.display_order.is_none() {
            node.display_order = Some(next);
            next = next.saturating_add(1);
        }
    }
}

/// The order the unnumbered nodes are numbered in: topological, with **declaration order** — the
/// position in `Stack.nodes` — as the tie-break between nodes that are ready at the same time.
///
/// The tie-break is the whole reason this is not [`tddy_core::changeset::Stack::topo_order`], whose
/// Kahn queue is seeded from a `BTreeMap` and so breaks ties by node id *lexicographically*: ten
/// independent nodes `n1…n10` come out `n1, n10, n2, …`. The web's `topoSortStackNodes` places every
/// ready node in declaration order, so that is the list a legacy plan was actually rendered as, and
/// the first number written must agree with it — otherwise the operator's first "Move up" swaps a
/// pair they never saw side by side and the list reshuffles under them.
fn numbering_order(stack: &tddy_core::changeset::Stack) -> Vec<String> {
    let known: std::collections::HashSet<&str> =
        stack.nodes.iter().map(|n| n.node_id.as_str()).collect();
    let mut placed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut remaining: Vec<&StackNode> = stack.nodes.iter().collect();
    let mut ordered: Vec<String> = Vec::with_capacity(stack.nodes.len());

    while !remaining.is_empty() {
        let (ready, blocked): (Vec<&StackNode>, Vec<&StackNode>) =
            remaining.iter().partition(|node| {
                node.parents
                    .iter()
                    .all(|p| !known.contains(p.as_str()) || placed.contains(p.as_str()))
            });
        if ready.is_empty() {
            // A cycle: every remaining node waits on another. Declaration order is an order the
            // caller can render, which beats dropping rows or returning none at all.
            ordered.extend(blocked.iter().map(|node| node.node_id.clone()));
            break;
        }
        for node in &ready {
            placed.insert(node.node_id.as_str());
            ordered.push(node.node_id.clone());
        }
        remaining = blocked;
    }
    ordered
}

/// One past the highest position any node records, or `0` for a stack that records none.
fn next_display_order(stack: &tddy_core::changeset::Stack) -> u32 {
    stack
        .nodes
        .iter()
        .filter_map(|node| node.display_order)
        .max()
        .map_or(0, |highest| highest.saturating_add(1))
}

/// The session's persisted stack, or an empty one when its changeset records none.
///
/// `op` prefixes the failure so the caller's name reaches the operator — every writer below reports
/// an unreadable changeset the same way.
fn read_stack(dir: &Path, op: &str) -> Result<tddy_core::changeset::Stack, String> {
    Ok(tddy_core::changeset::read_changeset(dir)
        .map_err(|e| format!("{op}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default())
}

/// Append one node to the persisted stack, numbering it last in the operator's reading order, and
/// answer the node as it was written.
///
/// Every append shares this, rather than spelling it out per writer, because the two statements in the
/// closure are a **stack invariant** and not this caller's business: [`assign_missing_display_order`]
/// numbers whatever the stack on disk left unnumbered, and only then is
/// [`next_display_order`] the position this node takes. A writer that forgot the first statement would
/// hand the new node a position an older node already holds, and the panel would order the two
/// arbitrarily.
///
/// Both decisions are made **inside** the `update_stack_atomic` closure, against the stack that is
/// about to be written: `update_stack_atomic` re-reads the file before applying its closure, so a
/// position chosen from a snapshot taken earlier could collide with a node another writer appended in
/// between.
fn append_node_atomic(dir: &Path, node: StackNode, op: &str) -> Result<StackNode, String> {
    let mut appended = node;
    tddy_core::changeset::update_stack_atomic(dir, |stack| {
        assign_missing_display_order(stack);
        appended.display_order = Some(next_display_order(stack));
        stack.nodes.push(appended.clone());
    })
    .map_err(|e| format!("{op}: failed to write stack: {e}"))?;
    Ok(appended)
}

/// Move one node one position up or down the operator's reading order, swapping positions with the
/// neighbour it passes.
///
/// `direction` is `"up"` or `"down"`. Moving past either end is a **successful no-op**: the control
/// at the end of the list is inert, not wrong, and refusing would make the operator's click an error
/// they have to read.
///
/// Touches nothing but `display_order` — `parents` is the dependency graph and this is the reading
/// order, and the whole point of the field is that the two move independently. An unknown node id is
/// refused before anything is written.
pub fn move_planned_pr_node(
    session_dir: &Path,
    node_id: &str,
    direction: &str,
) -> Result<tddy_core::changeset::Stack, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};
    const OP: &str = "move_planned_pr_node";

    let up = match direction {
        "up" => true,
        "down" => false,
        other => {
            return Err(format!(
                "{OP}: unknown direction '{other}' — expected 'up' or 'down'"
            ))
        }
    };

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    if stack.node(node_id).is_none() {
        return Err(format!("{OP}: node '{node_id}' not found"));
    }

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        swap_with_neighbour(stack, node_id, up);
    })
    .map_err(|e| format!("{OP}: failed to write stack: {e}"))?;

    read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to reload stack: {e}"))
        .map(|changeset| changeset.stack.unwrap_or_default())
}

/// Swap `node_id`'s position with the row above (`up`) or below it, or leave the stack alone when
/// there is no such row. Every node is numbered by the time this runs.
fn swap_with_neighbour(stack: &mut tddy_core::changeset::Stack, node_id: &str, up: bool) {
    let order = stack.display_order();
    let Some(position) = order.iter().position(|id| id == node_id) else {
        return;
    };
    // `get` answers `None` past either end, which is the no-op the control at the edge of the list
    // relies on.
    let neighbour_index = if up {
        position.checked_sub(1)
    } else {
        position.checked_add(1)
    };
    let Some(neighbour) = neighbour_index.and_then(|index| order.get(index)).cloned() else {
        return;
    };

    let position_of = |id: &str| stack.node(id).and_then(|node| node.display_order);
    let (Some(moved), Some(displaced)) = (position_of(node_id), position_of(&neighbour)) else {
        return;
    };
    for node in &mut stack.nodes {
        if node.node_id == node_id {
            node.display_order = Some(displaced);
        } else if node.node_id == neighbour {
            node.display_order = Some(moved);
        }
    }
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

/// Reject a parent list before anything is written: an id that resolves to no node, and — when
/// `node_id` names an *existing* node being rewritten — that node naming itself, or the same parent
/// named twice.
///
/// `node_id` is `None` for an append. The node does not exist yet, so it has no id for a parent to
/// collide with, and a repeated id is not rejected there — that is [`add_planned_pr_node`]'s and
/// [`adopt_pr_as_stack_node`]'s shipped behaviour, kept as it is.
///
/// `op` prefixes every message it emits, **except** that an empty `op` emits them bare. That
/// asymmetry is deliberate, not drift: [`add_planned_pr_node`] shipped `dangling parent ref: n9`
/// unprefixed and its callers read that exact string, so prefixing it now would change behaviour
/// already in use.
fn validate_parents(
    stack: &tddy_core::changeset::Stack,
    node_id: Option<&str>,
    parents: &[String],
    op: &str,
) -> Result<(), String> {
    let prefix = message_prefix(op);
    let mut seen: Vec<&String> = Vec::with_capacity(parents.len());
    for parent in parents {
        if node_id == Some(parent.as_str()) {
            return Err(format!("{prefix}node '{parent}' cannot be its own parent"));
        }
        if !stack.nodes.iter().any(|n| &n.node_id == parent) {
            return Err(format!("{prefix}dangling parent ref: {parent}"));
        }
        if node_id.is_some() && seen.contains(&parent) {
            return Err(format!("{prefix}parent '{parent}' is named more than once"));
        }
        seen.push(parent);
    }
    Ok(())
}

/// Reject the stack a mutation *would* produce when it is no longer a DAG.
///
/// Every writer here validates a candidate before touching the file, so a rejected call leaves the
/// stack on disk exactly as it was. `op` follows the same empty-means-unprefixed rule as
/// [`validate_parents`], for the same reason.
fn reject_if_cyclic(candidate: &tddy_core::changeset::Stack, op: &str) -> Result<(), String> {
    candidate
        .topo_order()
        .map_err(|e| format!("{}cycle detected: {e}", message_prefix(op)))?;
    Ok(())
}

/// `"{op}: "`, or `""` for an empty `op` — see [`validate_parents`] for why one writer emits its
/// rejections unprefixed.
fn message_prefix(op: &str) -> String {
    if op.is_empty() {
        String::new()
    } else {
        format!("{op}: ")
    }
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
    const OP: &str = "add_planned_pr_node";

    let existing = read_stack(session_dir, OP)?;

    // No `op` prefix: this rejection shipped bare and is read verbatim by its callers.
    validate_parents(&existing, None, &input.parents, "")?;

    let node_id = next_free_node_id(&existing);
    let new_node = StackNode {
        node_id,
        title: input.title,
        description: input.description,
        // A suggestion is a planned name, not a ref: `branch` stays empty until a child worktree
        // actually creates it (same contract as [`planned_prs_into_stack_nodes`]).
        branch: None,
        branch_suggestion: input.branch_suggestion,
        session_id: None,
        parents: input.parents,
        pr_status: None,
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
    };

    // Defense-in-depth cycle check: parents are restricted to pre-existing node ids above, so an
    // append alone can never actually cycle, but this keeps the same guard `validate_stack_plan`
    // applies to a whole plan, cheaply, rather than special-casing the append path as exempt.
    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    reject_if_cyclic(
        &tddy_core::changeset::Stack {
            version: existing.version,
            nodes: candidate_nodes,
        },
        "",
    )?;

    append_node_atomic(session_dir, new_node, OP)
}

/// Repoint a single planned node onto a new base.
///
/// Which parents survive depends on whether the caller names a target:
///
/// - `Some(target)` — retain exactly the parents whose `branch` is `target`, drop every other
///   parent. This is a retain rule, so a target that no parent owns drops all of them and the
///   node detaches onto `default_branch`. It is what makes a node stranded behind a merged-and-
///   deleted predecessor recoverable: that predecessor is still recorded as `open` in the plan
///   (the orchestrator agent writes `pr_status`), so no merged-parents rule could ever drop it.
/// - `None` — retain the parents that are not merged, i.e. drop merged parents only. The
///   behaviour for callers that do not name a target, such as the agent repoint.
///
/// The parent change is persisted atomically. The effective base branch (the nearest remaining
/// non-merged ancestor's branch, or `default_branch` when none remains) is then computed, the
/// node's local branch is rebased onto it and force-pushed, and the open GitHub PR's base is
/// re-targeted to it. Mirrors `bridge::execute_stack_repoint` applied to one node so the web
/// Repoint control and that agent path stay coherent. When the branch is not local (remote-only),
/// the git rebase is skipped and the PR base is still re-targeted.
///
/// `Some(target)` **collapses the node to a single parent** — the one owning `target` — or to none
/// when no parent owns it. Repointing is a decision to stack on one predecessor, so a multi-parent
/// node comes out of it single-parent by design; the other edges are dropped, not preserved.
///
/// `None` is the in-process drop-merged-parents mode. It is not reachable over the wire: the daemon
/// substitutes the project's resolved default branch for an empty `target_base_branch`, because a
/// client cannot always name that branch and forwarding the empty string would silently select this
/// different rule.
///
/// A node that owns no branch is a **plan-only** repoint: the parent change is persisted and the
/// updated node returned, with no rebase, no force-push and no PR re-target. There is nothing to
/// rebase and no PR of its own to re-target — and an unstarted node is precisely the one this
/// recovery exists for.
pub fn repoint_planned_pr_node(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    default_branch: &str,
    target_base_branch: Option<&str>,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrApi,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let changeset = read_changeset(session_dir)
        .map_err(|e| format!("repoint_planned_pr_node: failed to read changeset: {e}"))?;
    let stack = changeset.stack.unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("repoint_planned_pr_node: node '{node_id}' not found"))?
        .clone();

    // Which of the node's parents survive the repoint.
    //
    // Decided *inside* the `update_stack_atomic` closure, against the stack that is about to be
    // written. `update_stack_atomic` re-reads the file before applying its closure, and the
    // orchestrator agent writes the same file, so a set computed from the snapshot above would be
    // stale: a keep-list drops any parent added between the two reads, where the drop-list this
    // replaced would have kept it.
    let survives = |stack: &tddy_core::changeset::Stack, parent_id: &str| match target_base_branch {
        // A retain rule: only the parents that own the target base branch stay. A repoint therefore
        // *collapses* the node onto that one predecessor — or detaches it onto the default branch
        // when no parent owns the target, which is what a stranded node needs.
        Some(target) => stack
            .node(parent_id)
            .is_some_and(|p| p.branch.as_deref() == Some(target)),
        // No target named: drop only the parents that are known to have merged. Written as "not
        // known-merged" rather than "resolvable and not merged" so an unresolvable parent id is
        // kept, exactly as the drop-list form this replaced did.
        None => !stack.node(parent_id).is_some_and(|p| p.is_skipped()),
    };

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        let retained: Vec<String> = stack
            .node(node_id)
            .map(|n| {
                n.parents
                    .iter()
                    .filter(|parent_id| survives(stack, parent_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.parents = retained;
        }
    })
    .map_err(|e| format!("repoint_planned_pr_node: failed to persist stack: {e}"))?;

    // A node that owns no branch is a plan-only repoint: the persisted parent change above is the
    // whole effect, since there is nothing to rebase and no pull request of its own to re-target.
    if let Some(branch) = node.branch.as_deref() {
        realign_node_to_effective_base(
            "repoint_planned_pr_node",
            session_dir,
            repo_root,
            node_id,
            branch,
            default_branch,
            gh,
        )?;
    }

    let final_stack = read_changeset(session_dir)
        .map_err(|e| format!("repoint_planned_pr_node: failed to reload node: {e}"))?
        .stack
        .unwrap_or_default();
    final_stack
        .node(node_id)
        .cloned()
        .ok_or_else(|| format!("repoint_planned_pr_node: node '{node_id}' vanished after repoint"))
}

/// Bring a branch-owning node's git branch and GitHub PR in line with the parents now recorded for
/// it: rebase onto the new effective base, force-push with lease, re-target the open PR's base.
///
/// Reads the stack fresh rather than taking a base from the caller — the effective base is derived
/// from the parents that were *just written*, and a value computed before that write could name a
/// parent the write dropped.
///
/// A rebase conflict is recorded on the node as `pr_status.phase = "error"` carrying the git message
/// and then returned as an error: the branch is left mid-conflict for a human, and re-targeting the
/// PR to a base the branch does not sit on would misdescribe reality. When the branch is not local
/// (remote-only) the git half is skipped and the PR is still re-targeted.
///
/// Shared by [`repoint_planned_pr_node`] and [`set_stack_node_parents`], which differ only in how
/// they decide the parents — once the DAG is written, making reality match it is the same operation.
///
/// `op` is the caller's own name and prefixes every message this emits. They reach the operator
/// through the daemon, and naming a private helper the operator never invoked would describe the
/// failure of something they did not ask for.
fn realign_node_to_effective_base(
    op: &str,
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    branch: &str,
    default_branch: &str,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrApi,
) -> Result<(), String> {
    use crate::orchestrate_pr_stack::git_ops::{
        force_push_with_lease, local_branch_exists, merge_base, rebase_onto,
    };
    use tddy_core::changeset::read_changeset;

    // Effective base after the parent change: strip the `origin/` prefix so it names a branch usable
    // both as a rebase target and a GitHub PR base.
    let updated = read_changeset(session_dir)
        .map_err(|e| format!("{op}: failed to re-read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let base_ref = updated
        .effective_base_refs(node_id, default_branch)
        .into_iter()
        .next()
        .unwrap_or_else(|| default_branch.to_string());
    let effective_base = base_ref
        .strip_prefix("origin/")
        .unwrap_or(&base_ref)
        .to_string();

    // Rebase + force-push only when the branch is local; remote-only branches skip git ops.
    if local_branch_exists(repo_root, branch) {
        let old_base = merge_base(repo_root, branch, &effective_base)
            .unwrap_or_else(|_| effective_base.clone());
        if let Err(e) = rebase_onto(repo_root, &effective_base, &old_base, branch) {
            let err_msg = e.to_string();
            record_rebase_error(op, session_dir, node_id, &err_msg)?;
            return Err(format!(
                "{op}: rebase of {branch} onto {effective_base} failed: {err_msg}"
            ));
        }
        let expected_sha = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["rev-parse", branch])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let remote = tddy_core::worktree::detect_default_remote_name(repo_root)
            .unwrap_or_else(|| "origin".to_string());
        if let Err(e) = force_push_with_lease(repo_root, &remote, branch, &expected_sha) {
            log::warn!("{op}: force-push failed for {branch}: {e}");
        }
    }

    // Re-target the open PR's base to the effective base.
    if let Some(pr) = gh
        .get_open_pr(branch)
        .map_err(|e| format!("{op}: get_open_pr failed: {e}"))?
    {
        gh.patch_pr_base(pr.number, &effective_base)
            .map_err(|e| format!("{op}: patch_pr_base failed: {e}"))?;
    }
    Ok(())
}

/// Record a failed rebase on the node as `pr_status.phase = "error"` carrying the git message.
///
/// The branch is left mid-conflict for a human to finish, so the node has to say so: the stack is the
/// only place the operator will see it, and a node that still read `open` would describe a rebase that
/// succeeded.
fn record_rebase_error(
    op: &str,
    session_dir: &Path,
    node_id: &str,
    err_msg: &str,
) -> Result<(), String> {
    tddy_core::changeset::update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.pr_status = Some(tddy_core::changeset::GithubPrStatus {
                phase: "error".to_string(),
                url: None,
                error: Some(err_msg.to_string()),
            });
        }
    })
    .map_err(|e| format!("{op}: failed to record error: {e}"))
}

/// Which of a node's metadata fields an update rewrites. A field left `None` is untouched, which is
/// how a caller edits a title without having to restate the description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlannedPrInput {
    pub node_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub branch_suggestion: Option<String>,
}

/// What a deletion removed, and what it left behind.
///
/// `orphaned_branch` and `orphaned_session_id` are reported rather than cleaned up: deletion is a
/// *plan* operation, and silently deleting a branch or a child session would destroy work the
/// operator never asked to lose. Naming them lets the agent tell the operator what is now unowned.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedNode {
    pub node: StackNode,
    /// Ids of the children that inherited the removed node's parents.
    pub reparented_children: Vec<String>,
    pub orphaned_branch: Option<String>,
    pub orphaned_session_id: Option<String>,
}

/// The facts about an existing pull request that a stack node is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptedPrFacts {
    pub pull_number: u64,
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub url: String,
    /// `StackNode.pr_status.phase` vocabulary: `open` / `merged` / `closed`.
    pub phase: String,
}

/// Rewrite a node's `title`, `description` and/or `branch_suggestion`.
///
/// `title` and `description` are editable at any point in a node's life, including once it owns a
/// branch, a child session and an open PR — they are the plan's description of intent, and intent
/// gets clarified. `branch_suggestion` is not: once `branch` is set the suggestion has been
/// superseded by a real ref, and rewriting it would leave the plan claiming a name nothing uses.
///
/// An input naming no field at all is rejected rather than treated as a successful no-op: it can
/// only be a caller mistake, and reporting success would hide it.
///
/// Never touches `parents` (see [`set_stack_node_parents`]), `branch`, `session_id`, `pr_status` or
/// `internal_status`, and never contacts GitHub — pushing an edit to the PR is
/// [`sync_node_to_github_pr`], asked for separately.
pub fn update_planned_pr_node(
    session_dir: &Path,
    input: UpdatePlannedPrInput,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let node_id = input.node_id.clone();
    if input.title.is_none() && input.description.is_none() && input.branch_suggestion.is_none() {
        return Err(format!(
            "update_planned_pr_node: the update of node '{node_id}' names no field to change \
             (expected at least one of title, description, branch_suggestion)"
        ));
    }

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("update_planned_pr_node: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(&node_id)
        .ok_or_else(|| format!("update_planned_pr_node: node '{node_id}' not found"))?;
    if let (Some(branch), Some(suggestion)) =
        (node.branch.as_deref(), input.branch_suggestion.as_deref())
    {
        return Err(format!(
            "update_planned_pr_node: node '{node_id}' already owns branch '{branch}', so its \
             branch_suggestion cannot be rewritten to '{suggestion}'"
        ));
    }

    let mut updated: Option<StackNode> = None;
    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            if let Some(title) = input.title {
                node.title = title;
            }
            if let Some(description) = input.description {
                node.description = description;
            }
            if let Some(branch_suggestion) = input.branch_suggestion {
                node.branch_suggestion = Some(branch_suggestion);
            }
            updated = Some(node.clone());
        }
    })
    .map_err(|e| format!("update_planned_pr_node: failed to write stack: {e}"))?;

    // `update_stack_atomic` re-reads the file before applying the edit, so a node the check above saw
    // can still be gone by then — a concurrent writer removed it. Reporting that is not the same as
    // reporting a successful edit.
    updated.ok_or_else(|| {
        format!("update_planned_pr_node: node '{node_id}' vanished before the update was written")
    })
}

/// Push a node's title and/or body to its pull request, returning the PR number written to.
///
/// Takes the values explicitly rather than re-reading the node, so only the fields the operator
/// actually edited are sent — a `PATCH` that restates an unchanged body would overwrite whatever
/// was edited on GitHub in the meantime.
///
/// A node that records no PR is a rejection, not a skip: the caller asked for something that cannot
/// happen, and quietly doing nothing would be a fallback.
pub fn sync_node_to_github_pr(
    session_dir: &Path,
    node_id: &str,
    title: Option<&str>,
    body: Option<&str>,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrInsightApi,
) -> Result<u64, String> {
    let stack = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("sync_node_to_github_pr: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    // Resolving the number is also the "does this node have a PR at all" check: an unknown node and a
    // node that records no PR url both fail here, naming the node, before anything is sent.
    let number = crate::orchestrate_pr_stack::pr_insight::pull_number_for_node(&stack, node_id)
        .map_err(|e| format!("sync_node_to_github_pr: {e}"))?;

    gh.patch_pr_title_body(number, title, body).map_err(|e| {
        format!("sync_node_to_github_pr: patching PR #{number} for node '{node_id}' failed: {e}")
    })?;
    Ok(number)
}

/// Remove a node from the stack, reparenting its children onto that node's own parents.
///
/// Reparenting is what keeps the DAG whole. [`tddy_core::changeset::Stack::topo_order`] counts
/// in-degree only over parents that resolve to a node, so a parent id pointing at a removed node is
/// silently ignored by every existing check — a delete that simply dropped the node would leave the
/// stack quietly describing an edge that no longer exists. Children therefore inherit the removed
/// node's parents; a child that already lists one of them does not gain a duplicate. Deleting a root
/// leaves its children as roots, based off the stack bottom.
///
/// Refuses a node whose PR is **open**. Closing a PR is externally visible and is the agent's to ask
/// for explicitly via `pr_close`; a node whose PR is merged, closed, errored or absent deletes
/// freely.
///
/// The node's branch, worktree and child session are left untouched and reported — see
/// [`DeletedNode`].
pub fn delete_planned_pr_node(session_dir: &Path, node_id: &str) -> Result<DeletedNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("delete_planned_pr_node: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("delete_planned_pr_node: node '{node_id}' not found"))?;
    if node
        .pr_status
        .as_ref()
        .is_some_and(|status| status.phase == "open")
    {
        return Err(format!(
            "delete_planned_pr_node: node '{node_id}' has an open pull request — merge it with \
             pr_merge or close it with pr_close first"
        ));
    }

    // Validate the stack the delete *would* produce before writing anything. `topo_order` ignores
    // parent ids that resolve to no node, so a delete is the one mutation that could leave the stack
    // describing an edge nothing checks — the reparenting below is what keeps it whole.
    let mut candidate = stack.clone();
    remove_node_reparenting_children(&mut candidate, node_id);
    reject_if_cyclic(&candidate, "delete_planned_pr_node")?;

    let mut removed: Option<(StackNode, Vec<String>)> = None;
    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        removed = remove_node_reparenting_children(stack, node_id);
    })
    .map_err(|e| format!("delete_planned_pr_node: failed to write stack: {e}"))?;

    let (node, reparented_children) = removed.ok_or_else(|| {
        format!("delete_planned_pr_node: node '{node_id}' vanished before the delete was written")
    })?;
    Ok(DeletedNode {
        orphaned_branch: node.branch.clone(),
        orphaned_session_id: node.session_id.clone(),
        node,
        reparented_children,
    })
}

/// Remove `node_id` from `stack`, giving every child that listed it the removed node's parents
/// instead. Returns the removed node and the ids of the children that inherited, in stack order, or
/// `None` when the stack holds no such node.
///
/// A pure function of the stack so the same rule decides the candidate that is validated and the
/// write that is applied to the freshly-read stack — `update_stack_atomic` re-reads before applying
/// its closure, so a result computed from an earlier snapshot could describe a stack that was never
/// written.
///
/// A child that already lists an inherited parent keeps its single edge: `parents` is a set of
/// ancestors, and the same ancestor twice would make the node look like a two-parent merge.
fn remove_node_reparenting_children(
    stack: &mut tddy_core::changeset::Stack,
    node_id: &str,
) -> Option<(StackNode, Vec<String>)> {
    let position = stack.nodes.iter().position(|n| n.node_id == node_id)?;
    let removed = stack.nodes.remove(position);

    // Nothing here writes a node that lists itself as its own parent, but a stack on disk is written
    // by several processes and read back unvalidated. Inheriting such an entry verbatim would hand
    // every child a reference to the node just removed — and `topo_order` ignores parent ids that
    // resolve to no node, so nothing downstream would ever report it. Dropping it makes "no dangling
    // reference survives a delete" hold whatever the stack said.
    let inherited_parents: Vec<String> = removed
        .parents
        .iter()
        .filter(|parent| parent.as_str() != node_id)
        .cloned()
        .collect();

    let mut reparented = Vec::new();
    for child in &mut stack.nodes {
        if !child.parents.iter().any(|parent| parent == node_id) {
            continue;
        }
        let mut parents: Vec<String> = Vec::with_capacity(child.parents.len());
        for parent in &child.parents {
            let inherited: &[String] = if parent == node_id {
                &inherited_parents
            } else {
                std::slice::from_ref(parent)
            };
            for id in inherited {
                if !parents.contains(id) {
                    parents.push(id.clone());
                }
            }
        }
        child.parents = parents;
        reparented.push(child.node_id.clone());
    }
    Some((removed, reparented))
}

/// Set a node's parents outright, then bring git and GitHub in line with the new position.
///
/// This is the plan-level move, distinct from [`repoint_planned_pr_node`]: repointing answers "the
/// base branch drifted, retain the parent that owns this target", whereas this answers "the plan
/// changed, this node belongs *here* now". `parents` is the complete new set — an empty list makes
/// the node a root, based off the stack bottom.
///
/// Rejects an unknown parent id, a node naming itself, a repeated id, and any change that would
/// close a cycle. Nothing is written when validation fails, so a rejected call leaves the stack on
/// disk exactly as it was.
///
/// A node that owns no branch is a plan-only move: the persisted parent change is the whole effect.
/// A node that owns one is then realigned exactly as a repoint realigns it — rebased onto the new
/// effective base, force-pushed with lease, and its open PR re-targeted.
pub fn set_stack_node_parents(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    parents: &[String],
    default_branch: &str,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrApi,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};
    const OP: &str = "set_stack_node_parents";

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    if stack.node(node_id).is_none() {
        return Err(format!("{OP}: node '{node_id}' not found"));
    }

    validate_parents(&stack, Some(node_id), parents, OP)?;

    // Same guard `add_planned_pr_node` applies to an append: unlike an append, an arbitrary parent
    // rewrite really can close a cycle, so this one is load-bearing rather than defensive.
    let mut candidate = stack.clone();
    if let Some(candidate_node) = candidate.nodes.iter_mut().find(|n| n.node_id == node_id) {
        candidate_node.parents = parents.to_vec();
    }
    reject_if_cyclic(&candidate, OP)?;

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.parents = parents.to_vec();
        }
    })
    .map_err(|e| format!("{OP}: failed to write stack: {e}"))?;

    let moved = reload_moved_node(session_dir, node_id)?;

    // A node that owns no branch is a plan-only move: there is nothing to rebase and no pull request
    // of its own to re-target, so the persisted parent change is the whole effect.
    if let Some(branch) = moved.branch.as_deref() {
        realign_node_to_effective_base(
            OP,
            session_dir,
            repo_root,
            node_id,
            branch,
            default_branch,
            gh,
        )?;
    }

    Ok(moved)
}

/// Re-read the node a move just wrote, so [`set_stack_node_parents`] decides whether to realign from
/// the stack on disk rather than from its own pre-write snapshot.
///
/// `update_stack_atomic` re-reads before applying its closure, and another writer (`pr_spawn_child`,
/// or the web's start-session path through `link_stack_node_to_child_session`) can bind a branch to
/// this node in between. Gating on the snapshot's value would persist the new parents and silently
/// skip the rebase, the force-push and the PR re-target — while reporting success.
fn reload_moved_node(session_dir: &Path, node_id: &str) -> Result<StackNode, String> {
    const OP: &str = "set_stack_node_parents";
    tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to reload node: {e}"))?
        .stack
        .unwrap_or_default()
        .node(node_id)
        .cloned()
        .ok_or_else(|| format!("{OP}: node '{node_id}' vanished after the move"))
}

/// Append a stack node built from an existing pull request's facts.
///
/// Pure: the PR has already been read, so the DAG rules are testable without GitHub. Parents are
/// validated exactly as [`add_planned_pr_node`] validates them, and a PR already reachable through
/// some node — by its head branch or by the pull number recorded in its `pr_status.url` — is a
/// rejection: a PR must not be adopted twice.
///
/// The new node owns a `branch` and a `pr_status` from the start, but no `session_id`: an adopted PR
/// has a branch and a pull request, and no child session in this orchestrator. `internal_status` is
/// left unset for `pr_stack_status` to derive.
pub fn adopt_pr_as_stack_node(
    session_dir: &Path,
    facts: AdoptedPrFacts,
    parents: Vec<String>,
) -> Result<StackNode, String> {
    use tddy_core::changeset::GithubPrStatus;
    const OP: &str = "adopt_pr_as_stack_node";

    let existing = read_stack(session_dir, OP)?;

    validate_parents(&existing, None, &parents, OP)?;
    reject_if_branch_bound(&existing, &facts)?;
    reject_if_pull_number_tracked(&existing, &facts)?;

    let new_node = StackNode {
        node_id: next_free_node_id(&existing),
        title: facts.title,
        description: facts.body,
        // The branch is real from the start — the PR is already built on it. There is no suggestion
        // to record, since nothing here is going to choose a name.
        branch: Some(facts.head_branch),
        branch_suggestion: None,
        // An adopted PR has no child session in *this* orchestrator, and `internal_status` is
        // `pr_stack_status`'s to derive from the live PR.
        session_id: None,
        parents,
        pr_status: Some(GithubPrStatus {
            phase: facts.phase,
            // The url is how every existing caller recovers the node's PR number.
            url: Some(facts.url),
            error: None,
        }),
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
    };

    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    reject_if_cyclic(
        &tddy_core::changeset::Stack {
            version: existing.version,
            nodes: candidate_nodes,
        },
        OP,
    )?;

    append_node_atomic(session_dir, new_node, OP)
}

/// Refuse to adopt a PR whose head branch some node already owns — that node already *is* this PR in
/// the stack, and a second one would have `pr_merge` and `pr_stack_status` act on it twice.
fn reject_if_branch_bound(
    stack: &tddy_core::changeset::Stack,
    facts: &AdoptedPrFacts,
) -> Result<(), String> {
    const OP: &str = "adopt_pr_as_stack_node";
    let Some(bound) = stack
        .nodes
        .iter()
        .find(|n| n.branch.as_deref() == Some(facts.head_branch.as_str()))
    else {
        return Ok(());
    };
    Err(format!(
        "{OP}: branch '{}' is already bound to node '{}', so PR #{} is already tracked by this \
         stack",
        facts.head_branch, bound.node_id, facts.pull_number
    ))
}

/// Refuse to adopt a PR some node already records by number.
///
/// The recorded url is the system's only statement of "which pull request is this node" (see
/// `pr_number_from_status_url`), and a node can record one without ever recording a branch — a node
/// whose child session never ran, or one adopted before this check existed. Checking the branch alone
/// would let PR #42 be reached through two nodes, and `pr_merge` / `pr_stack_status` would then act on
/// it twice.
fn reject_if_pull_number_tracked(
    stack: &tddy_core::changeset::Stack,
    facts: &AdoptedPrFacts,
) -> Result<(), String> {
    const OP: &str = "adopt_pr_as_stack_node";
    let Some(tracking) = stack.nodes.iter().find(|n| {
        crate::orchestrate_pr_stack::pr_number_from_status_url(n.pr_status.as_ref())
            == Some(facts.pull_number)
    }) else {
        return Ok(());
    };
    Err(format!(
        "{OP}: PR #{} is already recorded on node '{}', so it is already tracked by this stack",
        facts.pull_number, tracking.node_id
    ))
}

/// Read a pull request and adopt it as a stack node — [`adopt_pr_as_stack_node`] with the fetch in
/// front of it.
///
/// Maps GitHub's live state onto `pr_status.phase`'s vocabulary. A **draft** is recorded as `open`:
/// the phase says whether the PR is still in play, and every consumer of `phase` (merge readiness,
/// delete's refusal, internal-status derivation) treats a draft as an open PR.
pub fn adopt_pr_into_stack(
    session_dir: &Path,
    pull_number: u64,
    parents: Vec<String>,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrInsightApi,
) -> Result<StackNode, String> {
    use crate::orchestrate_pr_stack::github::PrState;

    let pr = gh
        .get_pr(pull_number)
        .map_err(|e| format!("adopt_pr_into_stack: reading PR #{pull_number} failed: {e}"))?;
    let phase = match pr.state {
        PrState::Open | PrState::Draft => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
    };

    adopt_pr_as_stack_node(
        session_dir,
        AdoptedPrFacts {
            pull_number,
            title: pr.title,
            body: pr.body,
            head_branch: pr.head_branch,
            url: pr.url,
            phase: phase.to_string(),
        },
        parents,
    )
}

/// What a base session contributes to the root node of a stack seeded on it, once every rule that
/// governs whether it may seed one has been checked.
///
/// Answered by [`check_stack_seed_base`] so that a caller which only *validates* (the daemon, before
/// it spawns anything) and the caller that *writes* (the seeding function) derive the same facts from
/// the same rules, instead of each re-deriving them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededBase {
    /// The branch the seeded root node is bound to — and, through it, the base every descendant's
    /// worktree is created from.
    pub branch: String,
    /// The title the seeded row reads as: the base session's name, or its branch when it has none.
    pub title: String,
    /// The repository the base session records, when it records one. Consulted by callers that must
    /// keep a stack inside one repository; the seeding writer itself has no use for it.
    pub repo_path: Option<String>,
}

/// Why a session cannot be the base of a PR stack.
///
/// Carries the reason **and** which kind of refusal it is, because the daemon answers RPC callers in
/// a status vocabulary (`INVALID_ARGUMENT` for an id that names nothing, `FAILED_PRECONDITION` for a
/// session whose state is wrong) and must not re-derive the rules to pick a code. The message itself
/// is written once, here, so the CLI seeding path and the pre-spawn RPC refusal say the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackSeedBaseRefusal {
    /// The named session does not resolve to a readable changeset: the id itself is wrong.
    Unresolvable(String),
    /// The session resolves, but its state cannot seed a stack.
    Unusable(String),
}

impl StackSeedBaseRefusal {
    /// The operator-facing reason, identical for every caller.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Unresolvable(reason) | Self::Unusable(reason) => reason,
        }
    }
}

impl std::fmt::Display for StackSeedBaseRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

/// Whether `base_session_id` may be the base of a PR stack, and what it would contribute if it may.
///
/// The base session's half of the seeding preconditions, factored out of
/// [`seed_stack_with_base_session`] so it can be checked **before** anything is written or spawned:
///
/// - the daemon calls it before it spawns an orchestrator, so the new-session form can show the
///   reason instead of navigating to a session that came up unseeded;
/// - `tddy-coder`'s startup gate calls it before it creates the orchestrator's `changeset.yaml`, so a
///   refused seed leaves no changeset behind that would later resume as an ordinary *unseeded*
///   orchestrator;
/// - the seeding writer calls it as its own contract, since the CLI flag is reachable without the RPC.
///
/// The rules it enforces, all of them reads:
///
/// | Condition | Why it is refused |
/// |---|---|
/// | the session does not resolve to a readable changeset | there is nothing to bind a node to |
/// | it records no branch | the branch is the node's whole purpose, and a branchless base fails the spawn gate for every descendant |
/// | it is already a node of another orchestrator's stack | two orchestrators with repoint and pull authority over one branch is ambiguous ownership |
///
/// The caller-specific rules stay with their callers: whether the stack is already populated and
/// whether the base is the orchestrator itself need the orchestrator (see
/// [`check_stack_seed_not_self`]), and whether the repository matches needs the requesting project.
pub fn check_stack_seed_base(
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<SeededBase, StackSeedBaseRefusal> {
    let base_session_id = base_session_id.trim();
    let base_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_root, base_session_id);
    let base = tddy_core::changeset::read_changeset(&base_dir).map_err(|e| {
        StackSeedBaseRefusal::Unresolvable(format!(
            "base session '{base_session_id}' could not be read: {e}"
        ))
    })?;

    let branch = non_blank(base.branch.as_deref())
        .ok_or_else(|| {
            StackSeedBaseRefusal::Unusable(format!(
                "session '{base_session_id}' owns no branch, so there is nothing for the stack's \
                 root node to be bound to"
            ))
        })?
        .to_string();

    if let Some(owner) = non_blank(base.orchestrator_session_id.as_deref()) {
        return Err(StackSeedBaseRefusal::Unusable(format!(
            "session '{base_session_id}' is already a node of the stack orchestrated by '{owner}', \
             so a second stack cannot be based on it — two orchestrators with repoint and pull \
             authority over one branch is ambiguous ownership"
        )));
    }

    // A node with no legible title is the one thing the panel cannot render usefully, so an unnamed
    // base session is titled after the branch it works on.
    let title = non_blank(base.name.as_deref())
        .unwrap_or(branch.as_str())
        .to_string();

    Ok(SeededBase {
        branch,
        title,
        repo_path: non_blank(base.repo_path.as_deref()).map(str::to_string),
    })
}

/// Refuse a stack seeded on the very session that owns it.
///
/// Separate from [`check_stack_seed_base`] because it needs the orchestrator, which the daemon does
/// not have when it validates — the session does not exist yet. Both callers that *do* have it (the
/// seeding writer and `tddy-coder`'s startup gate) share this one wording.
pub fn check_stack_seed_not_self(
    orchestrator_dir: &Path,
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<(), String> {
    let base_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        sessions_root,
        base_session_id.trim(),
    );
    if is_same_session_dir(orchestrator_dir, &base_dir) {
        return Err(format!(
            "refusing to base the stack on itself — '{}' is the session that owns this stack",
            base_session_id.trim()
        ));
    }
    Ok(())
}

/// Append the single root node a new orchestrator's stack is seeded with, bound to a session the
/// operator already has open.
///
/// A **creation-time** act: every other way to populate a stack starts from something that does not
/// exist yet (a plan the agent writes, a planned node with no branch, a pull request), and this one
/// starts from work in flight. It refuses a stack that already has nodes rather than growing one —
/// growing a stack is [`add_planned_pr_node`]'s job, and a second seed would add a duplicate root.
///
/// The node owns a `branch` from the start, so `branch_suggestion` stays `None`: nothing here is
/// going to choose a name. `pr_status` stays `None` — whether that branch has a pull request is the
/// live-status poll's to discover, and recording `planned` for a branch that may already have an
/// open one would misreport it until the first tick.
///
/// Every refusal is raised before the write, so a refused seed leaves the changeset untouched.
pub fn seed_stack_with_base_session(
    orchestrator_dir: &Path,
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<StackNode, String> {
    const OP: &str = "seed_stack_with_base_session";

    check_stack_seed_not_self(orchestrator_dir, sessions_root, base_session_id)
        .map_err(|reason| format!("{OP}: {reason}"))?;

    let existing = read_stack(orchestrator_dir, OP)?;
    if !existing.nodes.is_empty() {
        return Err(format!(
            "{OP}: this stack already has {} node(s), so there is nothing to seed — a stack is \
             grown with add_planned_pr_node",
            existing.nodes.len()
        ));
    }

    // The base session's own half of the preconditions, shared with every other caller that has to
    // decide whether a session can seed a stack — so the rules, and their wording, exist once.
    let base = check_stack_seed_base(sessions_root, base_session_id)
        .map_err(|refusal| format!("{OP}: {refusal}"))?;

    let seeded = StackNode {
        node_id: next_free_node_id(&existing),
        title: base.title,
        description: String::new(),
        branch: Some(base.branch),
        branch_suggestion: None,
        session_id: Some(base_session_id.to_string()),
        // A seeded node is the root of the stack: nothing existed for it to be stacked on.
        parents: Vec::new(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
    };

    append_node_atomic(orchestrator_dir, seeded, OP)
}

/// The trimmed value, or `None` for an absent or blank one — proto3 and a hand-edited changeset both
/// carry "unset" as the empty string.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// Whether two paths name the same session directory.
///
/// Compares the canonical forms as well as the literal ones: the orchestrator's directory reaches
/// [`seed_stack_with_base_session`] as the path its own process was given, which need not be spelled
/// the way `sessions_root` spells it (a symlinked or relative sessions root).
fn is_same_session_dir(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// How a node's branch takes its base's commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseSyncStrategy {
    /// Add a merge commit. The default: it rewrites no history, needs no force-push, and disturbs
    /// none of the review anchors on the node's open pull request.
    #[default]
    Merge,
    /// Replay the branch's own commits on top of the base. Rewrites history, so it force-pushes
    /// with a lease — the right trade only when the operator chooses it.
    Rebase,
}

impl BaseSyncStrategy {
    /// The wire vocabulary: `"merge"` (also what an unnamed strategy means) or `"rebase"`.
    #[must_use]
    pub fn from_wire(strategy: &str) -> Self {
        match strategy.trim() {
            "rebase" => Self::Rebase,
            _ => Self::Merge,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
}

/// The `dirty_worktree_action` that commits and pushes outstanding tracked changes before pulling.
/// Anything else — including the empty default — refuses a dirty worktree instead.
const COMMIT_DIRTY_WORKTREE: &str = "commit";

/// What a pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullBaseReport {
    /// `"merge"` or `"rebase"` — the strategy that actually ran.
    pub strategy: &'static str,
    /// False when the branch already contained every commit on its base, in which case nothing was
    /// committed and nothing was pushed.
    pub changed: bool,
    /// The branch's tip after the pull.
    pub head_sha: String,
    pub pushed: bool,
    /// Why the push failed, when it did. The local work landed regardless — see
    /// [`pull_base_into_node_branch`].
    pub push_error: Option<String>,
}

/// Take a base branch's commits into a node's branch, inside that node's **own worktree**, and push
/// the result.
///
/// This is the operator's "stay where I am and take what the base has", distinct from a repoint
/// (which answers "this node belongs somewhere else now" by dropping parent edges).
///
/// The order of the checks is the safety design, because the button may be pressed while a child
/// session's agent is mid-turn in the worktree being touched:
///
/// 1. A node that does not exist, owns no branch, or was given no base is refused outright.
/// 2. A branch with no worktree is refused **by name**. It is deliberately not checked out in the
///    main repository instead: `git_ops::rebase_onto` does that, and it is a latent clobbering
///    hazard this does not extend. The worktree that is found must have this very branch checked
///    out — the resolver also answers with one that merely shares the branch's tip commit, which is
///    fine for displaying an indicator and would silently move a sibling's branch here.
/// 3. The worktree is checked for outstanding tracked changes **before the fetch** — before
///    anything touches git state at all. A refusal therefore leaves not just the tree but the
///    repository's remote-tracking refs exactly as they were. `dirty_worktree_action = "commit"`
///    commits those changes under `commit_message` and pushes them, then continues; anything else
///    refuses and names the paths. Untracked files are not outstanding work and never block.
/// 4. Only the base ref is fetched — never `git fetch <remote>` wholesale, which would also refresh
///    the remote-tracking ref a rebase's `--force-with-lease` is taken against and so turn "somebody
///    else pushed while you were rebasing" from a refused push into a clobbering one.
/// 5. A conflict aborts and is refused, naming the paths (PRD D33). The node is **not** stamped
///    `has-conflicts`: conflicts are a live fact on every poll now, so a persisted stamp can only go
///    stale, and clearing one risks stomping an agent's own `source: "override"`.
/// 6. A failed **push** is reported as `Ok` with `pushed = false` and a `push_error`, not as an
///    error (PRD D32). The local merge or rebase landed; rolling it back would be strictly worse
///    than saying so.
pub fn pull_base_into_node_branch(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    base_branch: &str,
    strategy: BaseSyncStrategy,
    dirty_worktree_action: &str,
    commit_message: &str,
) -> Result<PullBaseReport, String> {
    use crate::orchestrate_pr_stack::git_ops;
    const OP: &str = "pull_base_into_node_branch";

    let base_branch = base_branch.trim();
    if base_branch.is_empty() {
        return Err(format!(
            "{OP}: node '{node_id}' was given no base branch to pull from"
        ));
    }

    let stack = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("{OP}: node '{node_id}' not found"))?;
    let branch = node.branch.clone().ok_or_else(|| {
        format!(
            "{OP}: node '{node_id}' owns no branch yet, so there is nothing to pull '{base_branch}' \
             into — start its session first"
        )
    })?;

    let remote = tddy_core::worktree::detect_default_remote_name(repo_root)
        .unwrap_or_else(|| "origin".to_string());
    let base_branch = tddy_core::worktree::local_branch_name_for_remote(base_branch, &remote);

    // The pull runs in the node's own worktree. Checking the branch out anywhere else would move a
    // checkout the operator did not ask about.
    let worktree =
        tddy_core::worktree::worktree_path_for_branch(repo_root, &branch).ok_or_else(|| {
            format!(
                "{OP}: branch '{branch}' has no worktree checked out, so there is nowhere to pull \
                 '{base_branch}' into — start or resume the session that owns it first"
            )
        })?;
    // `worktree_path_for_branch` also answers with a worktree that merely *shares* the branch's tip
    // commit — a sibling node branched off this one and not yet committed resolves to exactly that.
    // Displaying such a worktree is harmless; merging into it would land this node's base on the
    // sibling's branch and then push this branch, which never moved, so git says "Everything
    // up-to-date" and the operator is told a pull happened that did not.
    let checked_out = tddy_core::worktree::checked_out_branch_name(&worktree).map_err(|e| {
        format!(
            "{OP}: could not read what '{}' has checked out: {e}",
            worktree.display()
        )
    })?;
    if checked_out.as_deref() != Some(branch.as_str()) {
        return Err(format!(
            "{OP}: the worktree found for '{branch}' ({}) has '{}' checked out, not '{branch}' — \
             pulling '{base_branch}' there would move the wrong branch; start or resume the session \
             that owns '{branch}' first",
            worktree.display(),
            checked_out.as_deref().unwrap_or("a detached HEAD")
        ));
    }

    // Before the fetch, before anything touches git state: a refusal here leaves the repository
    // byte-for-byte as it was found, remote-tracking refs included.
    let outstanding = git_ops::worktree_is_clean(&worktree)
        .map_err(|e| format!("{OP}: could not read the state of '{branch}': {e}"))?;
    if !outstanding.is_empty() {
        if dirty_worktree_action.trim() != COMMIT_DIRTY_WORKTREE {
            return Err(format!(
                "{OP}: the worktree for '{branch}' has uncommitted changes to {} — commit them, or \
                 re-run asking for them to be committed first",
                outstanding.join(", ")
            ));
        }
        commit_outstanding_work(OP, &worktree, &remote, &branch, commit_message)?;
    }

    git_ops::fetch_ref(&worktree, &remote, base_branch)
        .map_err(|e| format!("{OP}: could not fetch '{base_branch}' from '{remote}': {e}"))?;
    let base_ref = format!("{remote}/{base_branch}");
    if git_ops::ref_sha(&worktree, &base_ref).is_none() {
        return Err(format!(
            "{OP}: '{base_ref}' names no commit after fetching from '{remote}', so '{branch}' has \
             no base to take"
        ));
    }

    // Captured before the rewrite: the lease is the promise that the remote still holds what this
    // clone last saw, so a concurrent push aborts the force-push rather than being clobbered by it.
    //
    // `None` — no remote-tracking ref at all — is deliberately not an empty lease. Git reads
    // `--force-with-lease=<branch>:` as "the branch must be *absent* on the remote", so a branch that
    // is on the remote but has never been fetched into this clone would have every rebase push
    // refused with an opaque "stale info". There is nothing to take a lease against here, so the push
    // below falls back to a plain one, which the remote refuses unless it fast-forwards: a branch
    // this clone has never seen therefore either gets created or gets an honest non-fast-forward
    // rejection, and neither can clobber anyone's work.
    let lease_sha = match strategy {
        BaseSyncStrategy::Rebase => {
            git_ops::ref_sha(&worktree, &format!("refs/remotes/{remote}/{branch}"))
        }
        BaseSyncStrategy::Merge => None,
    };

    let outcome = match strategy {
        BaseSyncStrategy::Merge => git_ops::merge_ref_into_worktree(&worktree, &base_ref),
        BaseSyncStrategy::Rebase => git_ops::rebase_branch_onto_ref(&worktree, &base_ref),
    }
    .map_err(|e| {
        format!(
            "{OP}: could not {} '{base_ref}' into '{branch}': {e}",
            strategy.name()
        )
    })?;

    let head_sha = match outcome {
        // Already aborted by the primitive, so the worktree is exactly where it started.
        git_ops::SyncOutcome::Conflicted(paths) => {
            return Err(format!(
                "{OP}: '{branch}' conflicts with '{base_ref}' in {} — resolve them in the worktree \
                 (the {} was aborted and nothing was left half-applied)",
                paths.join(", "),
                strategy.name()
            ))
        }
        git_ops::SyncOutcome::AlreadyUpToDate => {
            return Ok(PullBaseReport {
                strategy: strategy.name(),
                changed: false,
                head_sha: git_ops::head_sha(&worktree)
                    .map_err(|e| format!("{OP}: could not read the tip of '{branch}': {e}"))?,
                pushed: false,
                push_error: None,
            })
        }
        git_ops::SyncOutcome::Applied(sha) => sha,
    };

    // A push that fails is reported, never rolled back: the merge or rebase is real local work, and
    // undoing it would be strictly worse than saying the remote does not have it yet.
    let push = match lease_sha {
        Some(lease_sha) => git_ops::force_push_with_lease(&worktree, &remote, &branch, &lease_sha),
        None => git_ops::push_branch(&worktree, &remote, &branch),
    };
    let push_error = push.err().map(|e| e.to_string());

    Ok(PullBaseReport {
        strategy: strategy.name(),
        changed: true,
        head_sha,
        pushed: push_error.is_none(),
        push_error,
    })
}

/// Commit the worktree's outstanding tracked changes and push them, so the pull starts from a clean
/// tree without any of the operator's work having been stashed, discarded or merged into.
fn commit_outstanding_work(
    op: &str,
    worktree: &Path,
    remote: &str,
    branch: &str,
    commit_message: &str,
) -> Result<(), String> {
    let commit_message = commit_message.trim();
    if commit_message.is_empty() {
        return Err(format!(
            "{op}: committing the outstanding changes in '{branch}' first needs a commit message"
        ));
    }
    crate::orchestrate_pr_stack::git_ops::commit_all_tracked(worktree, commit_message).map_err(
        |e| format!("{op}: could not commit the outstanding changes in '{branch}': {e}"),
    )?;
    crate::orchestrate_pr_stack::git_ops::push_branch(worktree, remote, branch).map_err(|e| {
        format!(
            "{op}: the outstanding changes in '{branch}' were committed but could not be pushed: \
             {e}"
        )
    })
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
            internal_status: None,
            display_order: None,
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

    // -----------------------------------------------------------------------
    // shared fixtures for the full-control primitives
    //
    // PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
    // Changeset: docs/dev/changesets.md (2026-07-30, pr-stack-full-control).
    // -----------------------------------------------------------------------

    /// [`a_node`] plus the things a started node owns: a branch, a child session, and a PR recorded
    /// at `phase`.
    fn a_started_node(node_id: &str, branch: &str, phase: &str, parents: Vec<&str>) -> StackNode {
        StackNode {
            branch: Some(branch.to_string()),
            session_id: Some(format!("session-for-{node_id}")),
            pr_status: Some(GithubPrStatus {
                phase: phase.to_string(),
                url: Some(format!("https://github.com/acme/repo/pull/{node_id}")),
                error: None,
            }),
            ..a_node(node_id, node_id, parents)
        }
    }

    fn write_stack(dir: &Path, nodes: Vec<StackNode>) {
        tddy_core::changeset::write_changeset(dir, &a_changeset_with_stack(nodes)).unwrap();
    }

    fn stack_on_disk(dir: &Path) -> Stack {
        read_changeset(dir).unwrap().stack.unwrap()
    }

    fn parents_of(dir: &Path, node_id: &str) -> Vec<String> {
        stack_on_disk(dir).node(node_id).unwrap().parents.clone()
    }

    fn node_ids(dir: &Path) -> Vec<String> {
        stack_on_disk(dir)
            .nodes
            .iter()
            .map(|n| n.node_id.clone())
            .collect()
    }

    /// A rejected call, so a test reads `assert_rejected(r).with_reason_containing("n9")`.
    struct Rejection(String);

    fn assert_rejected<T: std::fmt::Debug>(result: Result<T, String>) -> Rejection {
        match result {
            Err(reason) => Rejection(reason),
            Ok(value) => {
                panic!("expected the call to be rejected, but it succeeded with {value:?}")
            }
        }
    }

    impl Rejection {
        fn with_reason_containing(self, fragment: &str) -> Self {
            assert!(
                self.0.contains(fragment),
                "expected the rejection to mention '{fragment}', was '{}'",
                self.0
            );
            self
        }
    }

    fn an_update_of(node_id: &str) -> UpdatePlannedPrInput {
        UpdatePlannedPrInput {
            node_id: node_id.to_string(),
            ..UpdatePlannedPrInput::default()
        }
    }

    // -----------------------------------------------------------------------
    // update_planned_pr_node
    // -----------------------------------------------------------------------

    #[test]
    fn updating_a_nodes_title_and_description_persists_both_and_leaves_every_other_field_untouched()
    {
        // Given — n1 is fully started: it owns a branch, a child session and an open PR
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "open", vec![])],
        );
        let before = stack_on_disk(dir).node("n1").unwrap().clone();

        // When — the operator clarifies what the PR is for
        let node = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                title: Some("Token store, split out".to_string()),
                description: Some("Extracted from the parent PR.".to_string()),
                ..an_update_of("n1")
            },
        )
        .expect("editing intent on a started node should succeed");

        // Then — only the two fields named changed; the node's identity and reality did not
        assert_eq!(node.title, "Token store, split out");
        assert_eq!(node.description, "Extracted from the parent PR.");
        assert_eq!(node.branch, before.branch);
        assert_eq!(node.session_id, before.session_id);
        assert_eq!(node.pr_status, before.pr_status);
        assert_eq!(node.parents, before.parents);
        assert_eq!(
            stack_on_disk(dir).node("n1").unwrap().title,
            "Token store, split out"
        );
    }

    #[test]
    fn an_update_that_names_no_field_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "Original title", vec![])]);

        // When — every field left unset
        let result = update_planned_pr_node(dir, an_update_of("n1"));

        // Then — a no-op update can only be a caller mistake, so it is reported rather than hidden
        assert_rejected(result).with_reason_containing("names no field to change");
        assert_eq!(
            stack_on_disk(dir).node("n1").unwrap().title,
            "Original title"
        );
    }

    #[test]
    fn a_branch_suggestion_edit_is_accepted_while_the_node_is_still_planned() {
        // Given — n1 was never started, so its suggestion is still the only name it has
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                branch_suggestion: Some("feature/old-name".to_string()),
                ..a_node("n1", "Add token store", vec![])
            }],
        );

        // When
        let node = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                branch_suggestion: Some("feature/token-store".to_string()),
                ..an_update_of("n1")
            },
        )
        .expect("renaming a planned node's suggested branch should succeed");

        // Then — read back from disk as well as returned: an edit applied to a clone and never
        // persisted would be no edit at all
        assert_eq!(
            node.branch_suggestion.as_deref(),
            Some("feature/token-store")
        );
        assert_eq!(
            stack_on_disk(dir)
                .node("n1")
                .unwrap()
                .branch_suggestion
                .as_deref(),
            Some("feature/token-store")
        );
    }

    #[test]
    fn a_branch_suggestion_edit_is_rejected_once_the_node_owns_a_branch() {
        // Given — a worktree already exists, so the suggestion has been superseded by a real ref
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                branch_suggestion: Some("feature/suggested".to_string()),
                ..a_started_node("n1", "feature/actual", "open", vec![])
            }],
        );

        // When
        let result = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                branch_suggestion: Some("feature/renamed".to_string()),
                ..an_update_of("n1")
            },
        );

        // Then — refused, so the plan never claims a branch name nothing uses
        assert_rejected(result).with_reason_containing("feature/actual");
        assert_eq!(
            stack_on_disk(dir)
                .node("n1")
                .unwrap()
                .branch_suggestion
                .as_deref(),
            Some("feature/suggested")
        );
    }

    #[test]
    fn updating_an_unknown_node_id_is_rejected() {
        // Given — a stack with no node called n9
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "Add token store", vec![])]);

        // When
        let result = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                title: Some("Retitled".to_string()),
                ..an_update_of("n9")
            },
        );

        // Then
        assert_rejected(result).with_reason_containing("n9");
    }

    // -----------------------------------------------------------------------
    // delete_planned_pr_node
    // -----------------------------------------------------------------------

    #[test]
    fn deleting_a_middle_node_reparents_its_children_onto_that_nodes_parents() {
        // Given — a chain n1 → n2 → n3 where n2 turned out to be unnecessary
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n2"]),
            ],
        );

        // When
        let deleted =
            delete_planned_pr_node(dir, "n2").expect("deleting a planned node should succeed");

        // Then — n3 inherits n1, so the chain stays connected
        assert_eq!(deleted.node.node_id, "n2");
        assert_eq!(deleted.reparented_children, vec!["n3".to_string()]);
        assert_eq!(node_ids(dir), vec!["n1", "n3"]);
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
    }

    #[test]
    fn deleting_a_root_node_leaves_its_children_as_roots() {
        // Given — n1 is a root and n2 sits on it
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_node("n1", "one", vec![]), a_node("n2", "two", vec!["n1"])],
        );

        // When
        delete_planned_pr_node(dir, "n1").expect("deleting a root should succeed");

        // Then — n2 becomes a root itself, basing off the stack bottom
        assert_eq!(node_ids(dir), vec!["n2"]);
        assert_eq!(parents_of(dir, "n2"), Vec::<String>::new());
    }

    #[test]
    fn a_child_that_already_lists_the_inherited_parent_does_not_gain_a_duplicate() {
        // Given — a diamond: n3 depends on both n1 and n2, and n2 itself depends on n1
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n1", "n2"]),
            ],
        );

        // When — n2 is removed, so n3 would inherit n1 it already has
        delete_planned_pr_node(dir, "n2").expect("deleting a diamond's middle should succeed");

        // Then — one edge to n1, not two
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
    }

    #[test]
    fn no_reference_to_the_deleted_node_survives_in_any_nodes_parents() {
        // Given — two separate children both depend on n2
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n2"]),
                a_node("n4", "four", vec!["n2"]),
            ],
        );

        // When
        delete_planned_pr_node(dir, "n2").expect("deleting should succeed");

        // Then — a dangling parent ref would be silently ignored by `topo_order`, so none may remain
        let dangling: Vec<String> = stack_on_disk(dir)
            .nodes
            .iter()
            .filter(|n| n.parents.iter().any(|p| p == "n2"))
            .map(|n| n.node_id.clone())
            .collect();
        assert_eq!(dangling, Vec::<String>::new());
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
        assert_eq!(parents_of(dir, "n4"), vec!["n1".to_string()]);
    }

    #[test]
    fn deleting_a_node_whose_pr_is_open_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — n1's PR is open on GitHub
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "open", vec![])],
        );

        // When
        let result = delete_planned_pr_node(dir, "n1");

        // Then — closing a PR is externally visible and must be asked for explicitly
        assert_rejected(result).with_reason_containing("open");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[rstest]
    #[case::merged("merged")]
    #[case::closed("closed")]
    #[case::errored("error")]
    fn deleting_a_node_whose_pr_is_no_longer_open_is_allowed(#[case] phase: &str) {
        // Given — a node whose PR has already left the open state
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_started_node("n1", "feature/n1", phase, vec![])]);

        // When
        delete_planned_pr_node(dir, "n1").expect("a node with no open PR should delete");

        // Then
        assert_eq!(node_ids(dir), Vec::<String>::new());
    }

    #[test]
    fn deleting_a_started_node_reports_its_orphaned_branch_and_session_id() {
        // Given — n1 owns a branch and a child session, and its PR is already merged
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "merged", vec![])],
        );

        // When
        let deleted = delete_planned_pr_node(dir, "n1").expect("deleting should succeed");

        // Then — deletion is a plan operation, so what it left unowned is named, not removed
        assert_eq!(deleted.orphaned_branch.as_deref(), Some("feature/n1"));
        assert_eq!(
            deleted.orphaned_session_id.as_deref(),
            Some("session-for-n1")
        );
    }

    #[test]
    fn deleting_an_unknown_node_id_is_rejected() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "one", vec![])]);

        // When
        let result = delete_planned_pr_node(dir, "n9");

        // Then
        assert_rejected(result).with_reason_containing("n9");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    // -----------------------------------------------------------------------
    // adopt_pr_as_stack_node
    // -----------------------------------------------------------------------

    fn the_facts_of_pr(pull_number: u64, head_branch: &str) -> AdoptedPrFacts {
        AdoptedPrFacts {
            pull_number,
            title: format!("PR {pull_number}"),
            body: format!("body of PR {pull_number}"),
            head_branch: head_branch.to_string(),
            url: format!("https://github.com/acme/repo/pull/{pull_number}"),
            phase: "open".to_string(),
        }
    }

    #[test]
    fn adopting_a_pr_creates_a_node_carrying_its_head_branch_title_body_and_url() {
        // Given — an empty plan
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![]);

        // When
        let node = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![])
            .expect("adopting a PR should succeed");

        // Then
        assert_eq!(node.node_id, "n1");
        assert_eq!(node.title, "PR 77");
        assert_eq!(node.description, "body of PR 77");
        assert_eq!(node.branch.as_deref(), Some("feature/elsewhere"));
        let status = node.pr_status.as_ref().unwrap();
        assert_eq!(status.phase, "open");
        assert_eq!(
            status.url.as_deref(),
            Some("https://github.com/acme/repo/pull/77")
        );
        assert_eq!(stack_on_disk(dir).node("n1").unwrap().branch, node.branch);
    }

    #[test]
    fn an_adopted_node_starts_with_no_child_session_and_no_internal_status() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![]);

        // When
        adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![])
            .expect("adopting should succeed");

        // Then — as persisted: an adopted PR has a branch and a PR, and no child session in this
        // orchestrator. Read from disk rather than from the returned value, which a node built but
        // never written would satisfy just as well.
        let adopted = stack_on_disk(dir).node("n1").unwrap().clone();
        assert_eq!(adopted.session_id, None);
        assert_eq!(adopted.internal_status, None);
        assert_eq!(adopted.branch_suggestion, None);
    }

    #[test]
    fn adopting_a_pr_whose_head_branch_is_already_bound_to_a_node_is_rejected() {
        // Given — n1 already owns the branch this PR is built on
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/elsewhere", "open", vec![])],
        );

        // When
        let result = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![]);

        // Then — a PR must not be tracked twice
        assert_rejected(result).with_reason_containing("feature/elsewhere");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[test]
    fn adopting_a_pr_a_branchless_node_already_records_the_url_of_is_rejected() {
        // Given — n1 records PR #77 in its pr_status url but never recorded a branch of its own
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                pr_status: Some(GithubPrStatus {
                    phase: "open".to_string(),
                    url: Some("https://github.com/acme/repo/pull/77".to_string()),
                    error: None,
                }),
                ..a_node("n1", "one", vec![])
            }],
        );

        // When — the same pull request is adopted through a second node
        let result = adopt_pr_as_stack_node(dir, the_facts_of_pr(77, "feature/elsewhere"), vec![]);

        // Then — two nodes resolving to PR #77 would make pr_merge and pr_stack_status act on it twice
        assert_rejected(result).with_reason_containing("n1");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[test]
    fn adopting_a_pr_with_a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — a plan with no node called n9
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "one", vec![])]);

        // When
        let result = adopt_pr_as_stack_node(
            dir,
            the_facts_of_pr(77, "feature/elsewhere"),
            vec!["n9".to_string()],
        );

        // Then
        assert_rejected(result).with_reason_containing("n9");
        assert_eq!(node_ids(dir), vec!["n1"]);
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
