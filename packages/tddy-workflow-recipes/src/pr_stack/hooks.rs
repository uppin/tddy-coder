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
    analyze_stack_system_prompt, analyze_stack_user_prompt, write_stack_docs_system_prompt,
    write_stack_plan_system_prompt, write_stack_plan_user_prompt, StackPlanOutput,
    PR_STACK_PLAN_MD_BASENAME, STACK_PLAN_BASENAME,
};
use crate::SessionArtifactManifest;

use super::docs::{validate_stack_docs, write_stack_docs, StackDocsOutput};

/// Workflow name the shared planning prompts announce to the agent.
const RECIPE_NAME: &str = "pr-stack";

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
    context.set_sync("system_prompt", analyze_stack_system_prompt(RECIPE_NAME));
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
    context.set_sync("system_prompt", write_stack_plan_system_prompt(RECIPE_NAME));
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

/// `before_task` for `write-stack-docs`: seed the system/user prompt.
///
/// The user prompt names the planned nodes from the persisted stack, since the submit must cover
/// every one of them by id and the agent has no other list to work from.
fn before_write_stack_docs(context: &Context, session_dir: Option<&Path>) {
    context.set_sync("system_prompt", write_stack_docs_system_prompt(RECIPE_NAME));
    let stack = session_dir
        .and_then(|dir| read_changeset(dir).ok())
        .and_then(|cs| cs.stack);
    context.set_sync("prompt", write_stack_docs_user_prompt(stack.as_ref()));
}

/// The nodes to document, so the agent submits an entry for each by its exact `node_id`.
fn write_stack_docs_user_prompt(stack: Option<&Stack>) -> String {
    let mut prompt =
        String::from("Author the PRD and the changeset for every node in the planned stack.\n");
    let nodes = stack.map(|s| s.nodes.as_slice()).unwrap_or_default();
    if nodes.is_empty() {
        return prompt;
    }
    prompt.push_str("\n## Planned nodes\n\n");
    for node in nodes {
        let parents = if node.parents.is_empty() {
            "(root — off the stack base)".to_string()
        } else {
            node.parents.join(", ")
        };
        prompt.push_str(&format!(
            "- `{}` — {}\n  - parents: {parents}\n",
            node.node_id, node.title
        ));
        if !node.description.trim().is_empty() {
            prompt.push_str(&format!("  - description: {}\n", node.description.trim()));
        }
    }
    prompt
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

    // Both plan artifacts live under the artifacts root, beside exploration.md and the two
    // stack-status files: every wire reader resolves a manifest basename under `artifacts/`, so a
    // plan at the session root is invisible to them and unreachable as a host document.
    let artifacts_root = tddy_workflow::session_artifacts_root(dir);

    let yaml =
        serde_yaml::to_string(&plan).map_err(|e| format!("failed to serialize stack-plan: {e}"))?;
    tddy_core::atomic_file::write_atomic(&artifacts_root.join(STACK_PLAN_BASENAME), &yaml)
        .map_err(|e| format!("write {STACK_PLAN_BASENAME}: {e}"))?;

    let md = generate_pr_stack_plan_md(&plan);
    tddy_core::atomic_file::write_atomic(&artifacts_root.join(PR_STACK_PLAN_MD_BASENAME), &md)
        .map_err(|e| format!("write {PR_STACK_PLAN_MD_BASENAME}: {e}"))?;

    // Persist the optional code-discovery map to artifacts/exploration.md, reusing the same
    // helper and blank-gating as the tdd/bugfix planning recipes so it is surfaced as context.
    if let Some(exploration) = plan
        .exploration
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        crate::writer::write_exploration_file(&artifacts_root, exploration)
            .map_err(|e| format!("write exploration.md: {e}"))?;
    }

    set_changeset_state(dir, WorkflowState::new(super::STATE_STACK_PLANNED));
    Ok(())
}

/// `after_task` for `write-stack-docs`: parse the agent's YAML output, validate it against the
/// persisted stack, persist a `PRD.md` + `changeset.md` per node, and mark `StackDocsWritten`.
///
/// Validation runs before the first write, so a refused pass leaves the previous one — or an empty
/// artifacts root — exactly as it was. A half-written pass would leave an operator unable to tell
/// "not documented yet" from "deliberately empty".
fn after_write_stack_docs(
    dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let output: String = context
        .get_sync("output")
        .ok_or("write-stack-docs after_task requires output in context")?;

    let docs: StackDocsOutput = serde_yaml::from_str(&output)
        .map_err(|e| format!("failed to parse stack-docs YAML: {e}"))?;

    let changeset = read_changeset(dir).map_err(|e| format!("read changeset: {e}"))?;
    let stack = changeset
        .stack
        .ok_or("write-stack-docs requires a planned stack; none is persisted")?;

    validate_stack_docs(&stack, &docs)?;
    write_stack_docs(dir, &docs)?;

    set_changeset_state(dir, WorkflowState::new(super::STATE_STACK_DOCS_WRITTEN));
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
            "write-stack-docs" => before_write_stack_docs(context, session_dir.as_deref()),
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
            "write-stack-docs" => {
                let dir = session_dir
                    .ok_or("write-stack-docs after_task requires session_dir in context")?;
                after_write_stack_docs(&dir, context)?;
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
mod write_stack_docs_prompt_tests {
    use super::*;

    /// The rules have to reach the agent through the seeded `system_prompt`, not merely exist as a
    /// string constant — so this drives the real `before_task` seam, as the boundary-contract tests do.
    fn seeded_system_prompt() -> String {
        let ctx = Context::new();
        before_write_stack_docs(&ctx, None);
        ctx.get_sync::<String>("system_prompt")
            .expect("the docs hook must seed a system_prompt")
    }

    /// The Dependencies section is the whole anti-duplication mechanism: a child that is told only
    /// *that* it depends on n1 still has no idea which surfaces are n1's to create.
    #[test]
    fn the_prompt_requires_each_dependency_to_name_what_that_pr_delivers() {
        // Given / When
        let prompt = seeded_system_prompt();
        let lower = prompt.to_lowercase();

        // Then
        assert!(
            lower.contains("## dependencies"),
            "the prompt must name the Dependencies section; got:\n{prompt}"
        );
        assert!(
            lower.contains("deliver"),
            "the prompt must require each dependency to state what that PR delivers, not merely \
             that a dependency exists; got:\n{prompt}"
        );
        assert!(
            lower.contains("not enough"),
            "the prompt must rule out the mechanical answer — listing the parent's id and stopping \
             there — since that is what a dependency section degrades into; got:\n{prompt}"
        );
    }

    /// Without an API-plus-failing-tests contract the section degrades into "ship it sooner", which
    /// is advice rather than something a dependent can branch off.
    #[test]
    fn the_prompt_requires_a_draft_pr_contract_of_api_plus_failing_tests() {
        // Given / When
        let prompt = seeded_system_prompt();
        let lower = prompt.to_lowercase();

        // Then
        assert!(
            lower.contains("## draft pr contract"),
            "the prompt must name the Draft PR contract section; got:\n{prompt}"
        );
        assert!(
            lower.contains("failing test"),
            "the contract must be the API surface plus its failing tests; got:\n{prompt}"
        );
    }

    /// The step is delivered by a CLI invocation, not by a tool: `advertised_tools` deliberately
    /// excludes the `tddy-tools` subcommands, so an agent told to "use the `submit` tool with key
    /// `stack-docs`" searches a catalog that has never held one. The prompt spells the command out
    /// instead, as the plan prompt does.
    #[test]
    fn the_prompt_names_the_submit_command_an_agent_can_actually_run() {
        // Given / When
        let prompt = seeded_system_prompt();

        // Then
        assert!(
            prompt.contains("tddy-tools submit --goal write-stack-docs"),
            "prompt must name the exact CLI invocation; got:\n{prompt}"
        );
        assert!(
            prompt.contains("--data-stdin"),
            "prompt must ask for the heredoc/stdin form, not inline --data; got:\n{prompt}"
        );
        assert!(
            !prompt.contains("`submit` tool"),
            "prompt must not advertise a `submit` tool absent from the catalog; got:\n{prompt}"
        );
    }

    /// The prompt body is shared with its two planning siblings and carries a `{recipe}`
    /// placeholder, so a missed substitution would ship the placeholder itself to a live agent —
    /// the one failure the sharing introduces that the shared text cannot show.
    #[test]
    fn the_prompt_announces_the_pr_stack_recipe_by_name() {
        // Given / When
        let prompt = seeded_system_prompt();

        // Then
        assert!(
            prompt.contains("**pr-stack** workflow"),
            "prompt must announce the live recipe by name; got:\n{prompt}"
        );
        assert!(
            !prompt.contains("{recipe}"),
            "the recipe placeholder must be substituted, not shipped; got:\n{prompt}"
        );
    }

    #[test]
    fn the_prompt_names_every_required_section() {
        // Given / When
        let lower = seeded_system_prompt().to_lowercase();

        // Then — a section the prompt never mentions is one the validator will refuse for a reason
        // the agent was never told
        for heading in crate::pr_stack::REQUIRED_CHANGESET_HEADINGS {
            assert!(
                lower.contains(&heading.to_lowercase()),
                "the prompt must name the '{heading}' section the validator requires"
            );
        }
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
            dir.join("artifacts").join(STACK_PLAN_BASENAME).is_file(),
            "stack-plan.yaml must be written under the artifacts root"
        );
        assert!(
            dir.join("artifacts")
                .join(PR_STACK_PLAN_MD_BASENAME)
                .is_file(),
            "pr-stack-plan.md must be written under the artifacts root"
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

#[cfg(test)]
mod write_stack_plan_submit_instruction_tests {
    use super::*;

    /// The instruction has to reach the agent through the seeded `system_prompt`, not merely exist
    /// as a string constant — so each case drives the real `before_task` seam.
    fn seeded_write_stack_plan_prompt() -> String {
        let ctx = Context::new();
        before_write_stack_plan(&ctx, None);
        ctx.get_sync::<String>("system_prompt")
            .expect("hook must seed a system_prompt")
    }

    /// Session `01a04d4b-84f8-7fc0-b020-19ae73981175`: this prompt told the agent to submit "using
    /// the `submit` tool", which is not in the catalog — `advertised_tools` deliberately excludes
    /// the CLI subcommands. The agent searched every namespace for `submit`, got no matches, and
    /// guessed `tddy-tools-approval_prompt` before falling back to the shell. Every other recipe
    /// (bugfix analyze, tdd update-docs, tdd_small red) spells the command out instead.
    #[test]
    fn write_stack_plan_prompt_names_the_submit_command_an_agent_can_actually_run() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            prompt.contains("tddy-tools submit --goal write-stack-plan"),
            "prompt must name the exact CLI invocation, as the tdd and bugfix prompts do; \
             got: {prompt}"
        );
    }

    /// The prompt body is shared with the superseded plan-pr-stack recipe and carries a `{recipe}`
    /// placeholder, so a wrong constant or a missed substitution would ship the placeholder itself
    /// to a live agent — the one failure this sharing could introduce that the shared text cannot.
    #[test]
    fn write_stack_plan_prompt_announces_the_pr_stack_recipe_by_name() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            prompt.contains("**pr-stack** workflow"),
            "prompt must announce the live recipe by name; got: {prompt}"
        );
        assert!(
            !prompt.contains("{recipe}"),
            "the recipe placeholder must be substituted, not shipped; got: {prompt}"
        );
    }

    /// Only `Bash(tddy-tools *)` is auto-approved, so an agent that builds the JSON with `python3`
    /// or `cat` first — as the incident agent did — is one permission prompt away from stalling.
    /// The heredoc form also sidesteps shell escaping on a payload this size.
    #[test]
    fn write_stack_plan_prompt_tells_the_agent_to_pass_the_payload_on_stdin() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            prompt.contains("--data-stdin"),
            "prompt must ask for the heredoc/stdin form, not inline --data; got: {prompt}"
        );
    }

    /// Naming a tool that does not exist costs more than saying nothing: the agent trusts the name
    /// and burns its turns hunting for it. Neither the tool nor its invented `key` parameter is real.
    #[test]
    fn write_stack_plan_prompt_never_names_a_submit_tool_absent_from_the_catalog() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            !prompt.contains("`submit` tool"),
            "prompt must not advertise a `submit` tool; the CLI subcommands are not in the MCP \
             catalog (tddy-tools server.rs advertised_tools); got: {prompt}"
        );
        assert!(
            !prompt.contains("key `stack-plan`"),
            "prompt must not describe a `key` parameter; submit takes a goal and a JSON body; \
             got: {prompt}"
        );
    }

    /// "Also submit a human-readable plan summary using key `stack-plan-md`" asks for a turn that
    /// can never do anything: nothing reads `stack-plan-md`, and `after_write_stack_plan` generates
    /// the markdown itself via `generate_pr_stack_plan_md`.
    #[test]
    fn write_stack_plan_prompt_drops_the_stack_plan_md_submission_nothing_consumes() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            !prompt.contains("stack-plan-md"),
            "prompt must not request a second submission no code consumes; the hook derives the \
             markdown from the plan; got: {prompt}"
        );
    }

    /// The incident agent looked for its output in the repo's `artifacts/`, found nothing, and
    /// concluded the submit had failed. The plan and `exploration.md` land under the session dir,
    /// which only `$TDDY_SESSION_DIR` names.
    #[test]
    fn write_stack_plan_prompt_points_at_the_session_dir_where_the_plan_lands() {
        // Given / When
        let prompt = seeded_write_stack_plan_prompt();

        // Then
        assert!(
            prompt.contains("TDDY_SESSION_DIR"),
            "prompt must name the env var that locates the written artifacts; got: {prompt}"
        );
    }
}
