//! RunnerHooks for the unified pr-stack recipe.
//!
//! Combines the plan-phase prompt wiring (`analyze-stack` / `write-stack-plan`, matching
//! [`crate::plan_pr_stack::PlanPrStackHooks`]) with the orchestrate-loop stack-status rollup
//! (matching [`crate::orchestrate_pr_stack::OrchestratePrStackHooks`]), since both phases now
//! run in the same session under a single `RunnerHooks` implementation.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::changeset::{read_changeset, update_state, write_changeset, Stack};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::TaskResult;
use tddy_core::workflow::{clear_sinks, set_sinks};

use crate::orchestrate_pr_stack::{STACK_STATUS_JSON_BASENAME, STACK_STATUS_MD_BASENAME};
use crate::plan_pr_stack::{
    analyze_stack_user_prompt, write_stack_plan_user_prompt, StackPlanOutput,
    PR_STACK_PLAN_MD_BASENAME, STACK_PLAN_BASENAME,
};

pub struct PrStackHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl PrStackHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }

    fn agent_output_sink_impl(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }
}

fn session_dir_from_context(context: &Context) -> Option<PathBuf> {
    context
        .get_sync::<PathBuf>("session_dir")
        .or_else(|| context.get_sync::<PathBuf>("output_dir"))
}

fn set_changeset_state(session_dir: &Path, state: WorkflowState) {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, state);
        if let Err(e) = write_changeset(session_dir, &cs) {
            log::warn!("[pr-stack hooks] could not persist state: {e}");
        }
    }
}

fn analyze_stack_system_prompt() -> String {
    "You are assisting with the **pr-stack** workflow **analyze-stack** step.\n\n\
     ## Task: Analyze PR stack decomposition\n\n\
     Analyze the feature request and determine how to decompose it into a stack of pull requests. \
     Consider dependencies between PRs and identify which can be built in parallel (DAG structure).\n\n\
     This is a **read-only** analysis phase — do not write code or create files. \
     Focus on understanding the feature scope and identifying the optimal PR decomposition strategy, \
     noting which PRs depend on others and which can be developed concurrently.\n\n\
     For each proposed PR, identify:\n\
     1. A stable slug (`node_id`, e.g. `auth-store`, `api-client`)\n\
     2. A concise title\n\
     3. A description of what it implements\n\
     4. Its dependencies (which other PRs must merge first)\n\
     5. A branch name suggestion grouped under one shared stack namespace, \
     `feature/<stack-slug>/<node>` (e.g. `feature/auth/token-store`), so the stack's branches \
     group together\n\
     6. The child recipe to use (default: `tdd`)\n"
        .to_string()
}

fn write_stack_plan_system_prompt() -> String {
    "You are assisting with the **pr-stack** workflow **write-stack-plan** step.\n\n\
     ## Task: Emit structured PR stack plan\n\n\
     Based on the prior analysis, emit a structured PR stack plan using the `submit` tool \
     with key `stack-plan`. The YAML must conform to this contract:\n\n\
     ```yaml\n\
     version: 1\n\
     prs:\n\
       - node_id: n1          # stable slug, no spaces\n\
         title: \"Auth token store\"\n\
         description: \"Store tokens securely in the keyring\"\n\
         branch_suggestion: \"feature/auth/token-store\"\n\
         parents: []          # empty = root PR, off the stack base branch\n\
         child_recipe: tdd    # optional; default is tdd\n\
       - node_id: n2\n\
         title: \"Auth middleware\"\n\
         description: \"Validate tokens on each request\"\n\
         branch_suggestion: \"feature/auth/middleware\"\n\
         parents: [n1]        # depends on n1; use node_ids, not branch names\n\
     ```\n\n\
     **Validation rules** (the hook enforces these):\n\
     - `node_id` values must be unique\n\
     - All `parents` entries must reference an existing `node_id`\n\
     - The dependency graph must be acyclic (no cycles)\n\
     - Every `branch_suggestion` must be in `feature/<stack-slug>/<node>` form, and all PRs must \
     share the same `<stack-slug>` so the stack's branches group under one namespace \
     (e.g. `feature/auth/token-store`, `feature/auth/middleware`)\n\n\
     This may be the first time this plan is written, or a chat-driven refinement of an \
     already-written plan — in both cases, re-emit the full plan.\n\n\
     Also submit a human-readable plan summary using key `stack-plan-md`.\n"
        .to_string()
}

fn generate_pr_stack_plan_md(plan: &StackPlanOutput) -> String {
    let mut md = String::from("# PR Stack Plan\n\n");
    for pr in &plan.prs {
        md.push_str(&format!("## {} — {}\n\n", pr.node_id, pr.title));
        if !pr.description.trim().is_empty() {
            md.push_str(&pr.description);
            md.push_str("\n\n");
        }
        if let Some(ref branch) = pr.branch_suggestion {
            md.push_str(&format!("**Branch:** `{branch}`\n\n"));
        }
        if pr.parents.is_empty() {
            md.push_str("**Dependencies:** (root — off stack base)\n\n");
        } else {
            md.push_str(&format!("**Dependencies:** {}\n\n", pr.parents.join(", ")));
        }
        if let Some(ref recipe) = pr.child_recipe {
            md.push_str(&format!("**Recipe:** {recipe}\n\n"));
        }
    }
    md
}

fn stack_status_md(stack: &Stack) -> String {
    let mut md = String::from("# Stack Status\n\n");
    md.push_str("| Node | Title | Branch | Parents | PR Phase | Child State |\n");
    md.push_str("|------|-------|--------|---------|----------|-------------|\n");
    for node in &stack.nodes {
        let branch = node.branch.as_deref().unwrap_or("-");
        let parents = if node.parents.is_empty() {
            "(root)".to_string()
        } else {
            node.parents.join(", ")
        };
        let pr_phase = node
            .pr_status
            .as_ref()
            .map(|p| p.phase.as_str())
            .unwrap_or("-");
        let child_state = node.child_state.as_ref().map(|s| s.as_str()).unwrap_or("-");
        md.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} |\n",
            node.node_id, node.title, branch, parents, pr_phase, child_state
        ));
    }
    md
}

fn stack_status_json(stack: &Stack) -> Result<String, serde_json::Error> {
    let json_nodes: Vec<serde_json::Value> = stack
        .nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "node_id": node.node_id,
                "title": node.title,
                "branch": node.branch,
                "parents": node.parents,
                "pr_phase": node.pr_status.as_ref().map(|p| p.phase.as_str()),
                "pr_url": node.pr_status.as_ref().and_then(|p| p.url.as_deref()),
                "child_state": node.child_state.as_ref().map(|s| s.as_str()),
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "nodes": json_nodes,
        "updated_at": chrono::Utc::now().to_rfc3339(),
    }))
}

fn write_stack_status(
    session_dir: &Path,
    stack: &Stack,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let artifacts_dir = session_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)?;
    std::fs::write(
        artifacts_dir.join(STACK_STATUS_MD_BASENAME),
        stack_status_md(stack),
    )?;
    std::fs::write(
        artifacts_dir.join(STACK_STATUS_JSON_BASENAME),
        stack_status_json(stack)?,
    )?;
    Ok(())
}

/// Best-effort `stack-status.md`/`.json` rollup, run after every task tick regardless of which
/// task just ran. Logs and swallows failures — this is derived display data, never the
/// authoritative `Changeset.stack`.
fn refresh_stack_status_best_effort(context: &Context) {
    let Some(dir) = session_dir_from_context(context) else {
        return;
    };
    let Ok(cs) = read_changeset(&dir) else {
        return;
    };
    let Some(ref stack) = cs.stack else {
        return;
    };
    if let Err(e) = write_stack_status(&dir, stack) {
        log::warn!("[pr-stack hooks] write_stack_status failed: {e}");
    }
}

/// `before_task` for `analyze-stack`: seed the system/user prompt and mark the state.
fn before_analyze_stack(context: &Context, session_dir: Option<&Path>) {
    context.set_sync("system_prompt", analyze_stack_system_prompt());
    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    let answers: Option<String> = context.get_sync("answers");
    let user_prompt = if let Some(a) = answers.filter(|s| !s.trim().is_empty()) {
        format!(
            "{}\n\n## Clarification\n\n{a}",
            analyze_stack_user_prompt(&feature_input)
        )
    } else {
        analyze_stack_user_prompt(&feature_input)
    };
    context.set_sync("prompt", user_prompt);
    if let Some(dir) = session_dir {
        set_changeset_state(dir, WorkflowState::new("AnalyzeStack"));
    }
}

/// `before_task` for `write-stack-plan`: seed the system/user prompt and mark the state.
fn before_write_stack_plan(context: &Context, session_dir: Option<&Path>) {
    context.set_sync("system_prompt", write_stack_plan_system_prompt());
    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    let analysis_output: String = context.get_sync("output").unwrap_or_default();
    let answers: Option<String> = context.get_sync("answers");
    let user_prompt =
        write_stack_plan_user_prompt(&feature_input, &analysis_output, answers.as_deref());
    context.set_sync("prompt", user_prompt);
    if let Some(dir) = session_dir {
        set_changeset_state(dir, WorkflowState::new("WriteStackPlan"));
    }
}

/// `after_task` for `write-stack-plan`: parse the agent's YAML output, validate (or re-seed on a
/// refinement turn), persist `stack-plan.yaml` + `pr-stack-plan.md`, and mark `StackPlanned`.
fn after_write_stack_plan(
    dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let output: String = context
        .get_sync("output")
        .ok_or("write-stack-plan after_task requires output in context")?;

    let plan: StackPlanOutput = serde_yaml::from_str(&output)
        .map_err(|e| format!("failed to parse stack-plan YAML: {e}"))?;

    // Seed `Changeset.stack` from the plan on the first write, and re-seed it on every
    // subsequent refinement turn. `reseed_stack_from_plan_if_unspawned` validates the plan,
    // populates the stack from an empty/absent one, and refuses to overwrite once any node has
    // spawned a child session — so the `orchestrate` goal and its `pr_*` tools always operate on
    // a populated stack.
    super::reseed_stack_from_plan_if_unspawned(dir, &plan)?;

    let yaml =
        serde_yaml::to_string(&plan).map_err(|e| format!("failed to serialize stack-plan: {e}"))?;
    std::fs::write(dir.join(STACK_PLAN_BASENAME), &yaml)
        .map_err(|e| format!("write {STACK_PLAN_BASENAME}: {e}"))?;

    let md = generate_pr_stack_plan_md(&plan);
    std::fs::write(dir.join(PR_STACK_PLAN_MD_BASENAME), &md)
        .map_err(|e| format!("write {PR_STACK_PLAN_MD_BASENAME}: {e}"))?;

    set_changeset_state(dir, WorkflowState::new("StackPlanned"));
    Ok(())
}

impl RunnerHooks for PrStackHooks {
    fn on_enter_task(&self, _task_id: &str, _context: &Context) {
        set_sinks(self.agent_output_sink_impl(), None);
    }

    fn on_exit_task(&self, _task_id: &str, _context: &Context) {
        clear_sinks();
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[pr-stack hooks] before_task: {task_id}");
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        let session_dir = session_dir_from_context(context);

        match task_id {
            "analyze-stack" => before_analyze_stack(context, session_dir.as_deref()),
            "write-stack-plan" => before_write_stack_plan(context, session_dir.as_deref()),
            _ => {}
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[pr-stack hooks] after_task: {task_id}");
        let session_dir = session_dir_from_context(context);

        match task_id {
            "analyze-stack" => {
                if let Some(ref dir) = session_dir {
                    set_changeset_state(dir, WorkflowState::new("WriteStackPlan"));
                }
            }
            "write-stack-plan" => {
                let dir = session_dir
                    .ok_or("write-stack-plan after_task requires session_dir in context")?;
                after_write_stack_plan(&dir, context)?;
            }
            _ => {}
        }

        refresh_stack_status_best_effort(context);
        Ok(())
    }

    fn on_error(&self, task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::warn!("[pr-stack hooks] on_error task={task_id} err={error}");
        let Some(dir) = session_dir_from_context(context) else {
            return;
        };
        set_changeset_state(&dir, WorkflowState::new("Failed"));
    }
}
