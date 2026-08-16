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
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::task::TaskResult;
use tddy_core::workflow::{clear_sinks, set_sinks};

use crate::orchestrate_pr_stack::{STACK_STATUS_JSON_BASENAME, STACK_STATUS_MD_BASENAME};
use crate::plan_pr_stack::{
    analyze_stack_user_prompt, write_stack_plan_user_prompt, StackPlanOutput,
    PR_STACK_PLAN_MD_BASENAME, STACK_PLAN_BASENAME,
};
use crate::SessionArtifactManifest;

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
     ## Scoping rules: every PR is self-contained\n\n\
     Each PR must stand on its own — a reviewer can judge it, and it can merge, without waiting for \
     a later node in the stack. That means one node ships the **API/schema change, the code that \
     implements it, and its tests, together**.\n\n\
     **Do not split by layer.** These are the same node, never two:\n\
     - a schema/proto/interface change and the behavior behind it\n\
     - a backend endpoint and the handler that serves it\n\
     - a data model and the migration or persistence that uses it\n\
     - a function signature and its body; a type and its consumers\n\n\
     A node that only declares surface — new RPCs that return `unimplemented`, a field nothing \
     reads, a trait with stub impls — is **not** a valid PR. It cannot be reviewed for correctness \
     (there is no behavior to check), it cannot be tested beyond compiling, and it strands a \
     contract in the codebase that lies about what the system does.\n\n\
     **When a vertical slice feels too large, split by capability, not by layer.** Cut along \
     user-visible increments where each part is still end-to-end: one source variant instead of \
     all of them, one scope/enum case instead of the full set, one screen or one entry point, the \
     happy path before the edge cases. Each such PR ships its own contract plus behavior plus \
     tests, and the next one extends it.\n\n\
     **The only exceptions** — a node may omit implementation when it is:\n\
     - a purely mechanical rename, move, or extraction with no behavior change, or\n\
     - regenerating already-committed generated code, exposing no new surface.\n\n\
     If you believe a case warrants a third exception, put the reasoning in the PR's description \
     and let a human decide — do not invent one silently.\n\n\
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
     **Scoping rules** (your judgment — the hook cannot check these, so they are on you):\n\
     - Every PR is **self-contained**: the API/schema change, the code implementing it, and its \
     tests are one node. A node whose `description` promises only surface — new endpoints that \
     return `unimplemented`, a field nothing reads, stub impls — is not a valid PR.\n\
     - **Never split by layer** (schema then behavior, endpoint then handler, signature then body). \
     When a slice is too large, split by **capability**: one source variant, one enum case, one \
     screen, happy path before edge cases — each part still end-to-end.\n\
     - Sole exceptions: a mechanical rename/move with no behavior change, or regenerating \
     already-committed generated code with no new surface. Anything else, say so in the \
     `description` and let a human decide.\n\n\
     This may be the first time this plan is written, or a chat-driven refinement of an \
     already-written plan — in both cases, re-emit the full plan **and re-apply the scoping rules \
     above**: a refinement request must not talk you into a layer-split stack.\n\n\
     You may also include an optional top-level `exploration` field: a short markdown \
     code-discovery map of the key files you inspected, each with a `path:line` reference \
     (e.g. `- src/auth/store.rs:42 — token persistence`). When present it is persisted to \
     `artifacts/exploration.md` and surfaced as context to the orchestrate phase. Omit it if \
     there is nothing worth recording.\n\n\
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
    tddy_core::atomic_file::write_atomic(
        &artifacts_dir.join(STACK_STATUS_MD_BASENAME),
        stack_status_md(stack),
    )?;
    tddy_core::atomic_file::write_atomic(
        &artifacts_dir.join(STACK_STATUS_JSON_BASENAME),
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

/// `before_task` for `orchestrate`: prepend a context-reminder header pointing the agent at the
/// on-disk session artifacts (e.g. `exploration.md`), mirroring the tdd/bugfix recipes. When no
/// artifacts exist, [`prepend_context_header`] returns the prompt unchanged.
fn before_orchestrate(context: &Context, session_dir: Option<&Path>) {
    let Some(dir) = session_dir else {
        return;
    };
    let Some(prompt) = context.get_sync::<String>("prompt") else {
        return;
    };
    let basenames = super::PrStackRecipe.context_header_filenames();
    let repo_dir: Option<PathBuf> = context
        .get_sync("worktree_dir")
        .or_else(|| context.get_sync("output_dir"));
    let prompt = prepend_context_header(prompt, Some(dir), repo_dir.as_deref(), &basenames);
    context.set_sync("prompt", prompt);
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
    tddy_core::atomic_file::write_atomic(&dir.join(STACK_PLAN_BASENAME), &yaml)
        .map_err(|e| format!("write {STACK_PLAN_BASENAME}: {e}"))?;

    let md = generate_pr_stack_plan_md(&plan);
    tddy_core::atomic_file::write_atomic(&dir.join(PR_STACK_PLAN_MD_BASENAME), &md)
        .map_err(|e| format!("write {PR_STACK_PLAN_MD_BASENAME}: {e}"))?;

    // Persist the optional code-discovery map to artifacts/exploration.md, reusing the same
    // helper and blank-gating as the tdd/bugfix planning recipes so it is surfaced as context.
    if let Some(exploration) = plan
        .exploration
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        let artifacts_root = tddy_workflow::session_artifacts_root(dir);
        crate::writer::write_exploration_file(&artifacts_root, exploration)
            .map_err(|e| format!("write exploration.md: {e}"))?;
    }

    set_changeset_state(dir, WorkflowState::new(super::STATE_STACK_PLANNED));
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
            "orchestrate" => before_orchestrate(context, session_dir.as_deref()),
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

#[cfg(test)]
mod pr_boundary_scoping_rule_tests {
    use super::*;

    /// The scoping rule has to reach the agent through the seeded `system_prompt`, not merely exist
    /// as a string constant — so each case drives the real `before_task` seam.
    fn seeded_system_prompt(seed: fn(&Context, Option<&Path>)) -> String {
        let ctx = Context::new();
        seed(&ctx, None);
        ctx.get_sync::<String>("system_prompt")
            .expect("hook must seed a system_prompt")
    }

    /// A planning agent that splits a feature into "declare the API" then "implement it" produces
    /// exactly the stack this rule exists to prevent, so both planning prompts must forbid it.
    /// `write-stack-plan` is re-run on every chat-driven refinement — if the rule lived only in
    /// `analyze-stack`, refinement would quietly drop it.
    #[test]
    fn both_planning_prompts_require_self_contained_prs_and_forbid_a_layer_split() {
        for (goal, prompt) in [
            ("analyze-stack", seeded_system_prompt(before_analyze_stack)),
            (
                "write-stack-plan",
                seeded_system_prompt(before_write_stack_plan),
            ),
        ] {
            let lower = prompt.to_lowercase();

            assert!(
                lower.contains("self-contained"),
                "{goal} prompt must require each PR to be self-contained; got: {prompt}"
            );
            assert!(
                lower.contains("split by layer"),
                "{goal} prompt must name the layer-split anti-pattern; got: {prompt}"
            );
            assert!(
                lower.contains("capability"),
                "{goal} prompt must offer capability as the alternative split axis, so an \
                 oversized slice has somewhere to go; got: {prompt}"
            );
            assert!(
                lower.contains("unimplemented"),
                "{goal} prompt must reject a surface-only node (stubs returning unimplemented); \
                 got: {prompt}"
            );
            assert!(
                lower.contains("rename") && lower.contains("generated code"),
                "{goal} prompt must state the two narrow exceptions so the agent does not invent \
                 its own; got: {prompt}"
            );
        }
    }
}

#[cfg(test)]
mod exploration_and_context_tests {
    use super::*;
    use std::fs;
    use tddy_core::changeset::{write_changeset, Changeset};

    /// A valid `write-stack-plan` submission (single root PR, branch under one namespace) that also
    /// carries a code-discovery `exploration` doc — the pr-stack analogue of the tdd/bugfix planning
    /// submissions that persist `artifacts/exploration.md`.
    fn submit_yaml_with_exploration() -> String {
        r##"version: 1
exploration: "# Exploration\n- src/lib.rs:1 entry point"
prs:
  - node_id: n1
    title: Root PR
    description: root
    branch_suggestion: feature/auth/root
    parents: []
"##
        .to_string()
    }

    fn a_planned_session_context(dir: &Path, submit_yaml: String) -> Context {
        write_changeset(dir, &Changeset::default()).expect("seed changeset");
        let ctx = Context::new();
        ctx.set_sync("output", submit_yaml);
        ctx
    }

    // ── Milestone A: pr-stack persists exploration.md from the write-stack-plan submit ──

    #[test]
    fn write_stack_plan_writes_exploration_md_under_artifacts_when_submitted() {
        // Given — a completed write-stack-plan submission carrying an exploration doc
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let ctx = a_planned_session_context(dir, submit_yaml_with_exploration());

        // When — the after_task hook persists the plan
        after_write_stack_plan(dir, &ctx).expect("after_write_stack_plan");

        // Then — exploration.md lands under artifacts/ carrying the submitted code map
        let exploration = dir.join("artifacts").join("exploration.md");
        assert!(
            exploration.is_file(),
            "expected exploration.md at {}",
            exploration.display()
        );
        let content = fs::read_to_string(&exploration).unwrap();
        assert!(
            content.contains("src/lib.rs:1 entry point"),
            "exploration.md must contain the submitted code map; got:\n{content}"
        );
    }

    #[test]
    fn write_stack_plan_writes_no_exploration_md_when_the_field_is_blank() {
        // Given — a submission whose exploration field is whitespace-only
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let ctx = a_planned_session_context(
            dir,
            r#"version: 1
exploration: "   "
prs:
  - node_id: n1
    title: Root PR
    branch_suggestion: feature/auth/root
    parents: []
"#
            .to_string(),
        );

        // When
        after_write_stack_plan(dir, &ctx).expect("after_write_stack_plan");

        // Then — no exploration.md is written for a blank field
        assert!(
            !dir.join("artifacts").join("exploration.md").exists(),
            "no exploration.md must be written when the exploration field is blank"
        );
    }

    #[test]
    fn write_stack_plan_still_persists_stack_plan_yaml_and_md_alongside_exploration() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let ctx = a_planned_session_context(dir, submit_yaml_with_exploration());

        // When
        after_write_stack_plan(dir, &ctx).expect("after_write_stack_plan");

        // Then — adding exploration.md does not disturb the two pre-existing plan artifacts
        assert!(
            dir.join(STACK_PLAN_BASENAME).is_file(),
            "stack-plan.yaml must still be written to the session root"
        );
        assert!(
            dir.join(PR_STACK_PLAN_MD_BASENAME).is_file(),
            "pr-stack-plan.md must still be written to the session root"
        );
        assert!(
            dir.join("artifacts").join("exploration.md").is_file(),
            "exploration.md must be written alongside the plan artifacts"
        );
    }

    // ── Milestone B: exploration.md is surfaced to the orchestrate goal as context ──

    #[test]
    fn orchestrate_prompt_surfaces_the_exploration_doc_path_via_context_reminder() {
        // Given — a planned session whose exploration.md already exists on disk
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let artifacts = dir.join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        fs::write(
            artifacts.join("exploration.md"),
            "# Exploration\n- src/lib.rs:1 entry point\n",
        )
        .unwrap();

        let hooks = PrStackHooks::new(None);
        let ctx = Context::new();
        ctx.set_sync("session_dir", dir.to_path_buf());
        ctx.set_sync("prompt", "resolve the stack".to_string());

        // When — the interactive orchestrate turn is prepared
        hooks
            .before_task("orchestrate", &ctx)
            .expect("before_task orchestrate");

        // Then — the agent is pointed at exploration.md via the context-reminder header
        let prompt: String = ctx.get_sync("prompt").expect("prompt in context");
        assert!(
            prompt.contains("<context-reminder>"),
            "orchestrate prompt must carry a context-reminder header; got:\n{prompt}"
        );
        assert!(
            prompt.contains("exploration.md"),
            "orchestrate prompt must reference exploration.md; got:\n{prompt}"
        );
    }

    #[test]
    fn orchestrate_prompt_has_no_context_reminder_when_no_docs_exist() {
        // Given — a session with an empty artifacts dir (no context docs written yet)
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::create_dir_all(dir.join("artifacts")).unwrap();

        let hooks = PrStackHooks::new(None);
        let ctx = Context::new();
        ctx.set_sync("session_dir", dir.to_path_buf());
        ctx.set_sync("prompt", "resolve the stack".to_string());

        // When
        hooks
            .before_task("orchestrate", &ctx)
            .expect("before_task orchestrate");

        // Then — no header is injected when there is nothing to reference
        let prompt: String = ctx.get_sync("prompt").expect("prompt in context");
        assert!(
            !prompt.contains("<context-reminder>"),
            "no context-reminder header should be injected when no docs exist; got:\n{prompt}"
        );
    }
}
