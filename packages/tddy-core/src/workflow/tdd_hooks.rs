//! TddWorkflowHooks — file I/O and event emission for the TDD workflow.
//!
//! Implements RunnerHooks for the graph-flow path. Writes artifacts from context
//! in after_task, reads artifacts into context in before_task.

use crate::backend::{AgentOutputSink, ProgressSink};
use crate::changeset::{
    append_session_and_update_state, get_session_for_tag, next_goal_for_state, read_changeset,
    resolve_model, update_state, write_changeset, Changeset, SessionEntry,
};
use crate::error::WorkflowError;
use crate::output::slugify_directory_name;
use crate::output::{
    parse_acceptance_tests_response, parse_evaluate_response, parse_green_response,
    parse_planning_response_with_base, parse_red_response, parse_refactor_response,
    parse_update_docs_response, parse_validate_subagents_response, update_acceptance_tests_file,
    update_progress_file, write_acceptance_tests_file, write_artifacts, write_demo_results_file,
    write_evaluation_report, write_progress_file, write_red_output_file, PlanningOutput,
};
use crate::presenter::WorkflowEvent;
use crate::stream::ProgressEvent as StreamProgressEvent;
use crate::workflow::context::Context;
use crate::workflow::graph::ElicitationEvent;
use crate::workflow::hooks::RunnerHooks;
use crate::workflow::task::TaskResult;
use crate::workflow::{
    acceptance_tests, demo, evaluate, green, prepend_context_header, red, refactor, update_docs,
    validate_subagents,
};
use crate::{setup_worktree_for_session, workflow::find_git_root};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Hooks for the TDD workflow. Handles file I/O. Event emission for TUI when event_tx is set.
pub struct TddWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl TddWorkflowHooks {
    /// Create hooks for CLI path (file I/O only, no events).
    pub fn new() -> Self {
        Self { event_tx: None }
    }

    /// Create hooks with event emission for TUI (GoalStarted, StateChange).
    pub fn with_event_tx(event_tx: mpsc::Sender<WorkflowEvent>) -> Self {
        Self {
            event_tx: Some(event_tx),
        }
    }
}

impl Default for TddWorkflowHooks {
    fn default() -> Self {
        Self::new()
    }
}

/// Active agent CLI thread id from `changeset.yaml`: `state.session_id`, else tagged `impl`
/// session, else `.impl-session` (same rules as `before_green`).
fn resolve_agent_session_id(plan_dir: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let changeset =
        read_changeset(plan_dir).map_err(|e| -> Box<dyn Error + Send + Sync> { Box::new(e) })?;
    changeset
        .state
        .session_id
        .clone()
        .or_else(|| get_session_for_tag(&changeset, "impl"))
        .or_else(|| {
            std::fs::read_to_string(plan_dir.join(".impl-session"))
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "agent session id missing: need changeset.state.session_id, impl session, or .impl-session",
            ))
        })
}

fn before_plan(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    use crate::output::create_session_dir_with_id;
    // Resolve actual plan_dir: session_base+session_id, or output_dir/slug, or fallback.
    let dir: PathBuf = if let (Some(base), Some(sid)) = (
        context.get_sync::<PathBuf>("session_base"),
        context.get_sync::<String>("session_id"),
    ) {
        create_session_dir_with_id(&base, &sid).map_err(|e| e.to_string())?
    } else if let (Some(output_dir), Some(feature_input)) = (
        context.get_sync::<PathBuf>("output_dir"),
        context.get_sync::<String>("feature_input"),
    ) {
        let input = feature_input.trim();
        if !input.is_empty() {
            output_dir.join(slugify_directory_name(input))
        } else {
            plan_dir.to_path_buf()
        }
    } else {
        plan_dir.to_path_buf()
    };
    // Create changeset if missing (session_base path or tests that bypass entry paths).
    if read_changeset(&dir).is_err() {
        let _ = std::fs::create_dir_all(&dir);
        let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
        let repo_path = context
            .get_sync::<PathBuf>("output_dir")
            .map(|p| p.display().to_string());
        let init_cs = Changeset {
            initial_prompt: Some(feature_input),
            repo_path,
            ..Changeset::default()
        };
        let _ = write_changeset(&dir, &init_cs);
    }
    let mut cs = read_changeset(&dir).map_err(|e| e.to_string())?;
    update_state(&mut cs, "Planning");
    let _ = write_changeset(&dir, &cs);
    Ok(())
}

/// Ensure worktree exists before acceptance-tests. Creates from origin/master if needed.
/// Sets worktree_dir in context; sends WorktreeSwitched when event_tx is set.
/// Skips worktree creation for stub backend (demo): uses output_dir directly.
fn ensure_worktree_for_acceptance_tests(
    plan_dir: &Path,
    context: &Context,
    event_tx: Option<&mpsc::Sender<WorkflowEvent>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if context.get_sync::<PathBuf>("worktree_dir").is_some() {
        return Ok(());
    }
    let cs = read_changeset(plan_dir).map_err(|e| e.to_string())?;
    if let Some(ref wt) = cs.worktree {
        context.set_sync("worktree_dir", PathBuf::from(wt));
        return Ok(());
    }
    let output_dir: PathBuf = context
        .get_sync("output_dir")
        .ok_or("output_dir required for worktree creation")?;

    let backend_name = context
        .get_sync::<String>("backend_name")
        .unwrap_or_default();
    if backend_name == "stub" {
        log::debug!(
            "[tddy-core] acceptance-tests: stub backend, using output_dir as worktree (no git fetch)"
        );
        context.set_sync("worktree_dir", output_dir.clone());
        if let Some(tx) = event_tx {
            let _ = tx.send(WorkflowEvent::WorktreeSwitched { path: output_dir });
        }
        return Ok(());
    }

    let repo_root = find_git_root(&output_dir);
    match setup_worktree_for_session(&repo_root, plan_dir) {
        Ok(worktree_path) => {
            context.set_sync("worktree_dir", worktree_path.clone());
            if let Some(tx) = event_tx {
                let _ = tx.send(WorkflowEvent::WorktreeSwitched {
                    path: worktree_path,
                });
            }
            Ok(())
        }
        Err(e) => {
            log::error!(
                "[tddy-core] worktree creation failed: repo_root={:?}, plan_dir={:?}, error={}",
                repo_root,
                plan_dir,
                e
            );
            Err(format!("worktree creation failed: {}", e).into())
        }
    }
}

fn before_acceptance_tests(
    plan_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = std::fs::read_to_string(plan_dir.join("PRD.md"))
        .map_err(|e| format!("read PRD.md: {}", e))?;
    let changeset = read_changeset(plan_dir).map_err(|e| e.to_string())?;
    let model = resolve_model(
        Some(&changeset),
        "acceptance-tests",
        context.get_sync::<String>("model").as_deref(),
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => acceptance_tests::build_followup_prompt(&prd, a),
        None => acceptance_tests::build_prompt(&prd),
    };
    let repo_dir: Option<PathBuf> = context
        .get_sync("worktree_dir")
        .or_else(|| context.get_sync("output_dir"));
    let prompt = prepend_context_header(prompt, Some(plan_dir), repo_dir.as_deref());
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", acceptance_tests::system_prompt());
    // Plan-mode sessions cannot be resumed with acceptEdits; create fresh session.
    let session_id = uuid::Uuid::new_v4().to_string();
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", false);
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "AcceptanceTesting");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_red(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = std::fs::read_to_string(plan_dir.join("PRD.md"))
        .map_err(|e| format!("read PRD.md: {}", e))?;
    let at = std::fs::read_to_string(plan_dir.join("acceptance-tests.md"))
        .map_err(|e| format!("read acceptance-tests.md: {}", e))?;
    let changeset = read_changeset(plan_dir).ok();
    let model = resolve_model(
        changeset.as_ref(),
        "red",
        context.get_sync::<String>("model").as_deref(),
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => red::build_followup_prompt(&prd, &at, a),
        None => red::build_prompt(&prd, &at),
    };
    let repo_dir: Option<PathBuf> = context
        .get_sync("worktree_dir")
        .or_else(|| context.get_sync("output_dir"));
    let prompt = prepend_context_header(prompt, Some(plan_dir), repo_dir.as_deref());
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", red::system_prompt());
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("model", model);
    let session_id = uuid::Uuid::new_v4().to_string();
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", false);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "RedTesting");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_green(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let progress = std::fs::read_to_string(plan_dir.join("progress.md"))
        .map_err(|e| format!("read progress.md: {}", e))?;
    let prd = std::fs::read_to_string(plan_dir.join("PRD.md")).ok();
    let at = std::fs::read_to_string(plan_dir.join("acceptance-tests.md")).ok();
    let changeset = read_changeset(plan_dir).ok();
    let session_id = resolve_agent_session_id(plan_dir).map_err(|e| {
        format!(
            "green requires changeset with state.session_id, impl session, or .impl-session file: {}",
            e
        )
    })?;
    let model = resolve_model(
        changeset.as_ref(),
        "green",
        context.get_sync::<String>("model").as_deref(),
    );
    let run_demo = context.get_sync::<bool>("run_demo").unwrap_or(false);
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => green::build_followup_prompt(&progress, a, prd.as_deref(), at.as_deref()),
        None => green::build_prompt(&progress, prd.as_deref(), at.as_deref()),
    };
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", green::system_prompt(run_demo));
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "GreenImplementing");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_demo(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let demo_plan = std::fs::read_to_string(plan_dir.join("demo-plan.md"))
        .map_err(|e| format!("read demo-plan.md: {}", e))?;
    let prompt = format!(
        "Execute the demo described in demo-plan.md:\n\n{}",
        demo_plan
    );
    let session_id = resolve_agent_session_id(plan_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", demo::system_prompt());
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "DemoRunning");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_evaluate(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = std::fs::read_to_string(plan_dir.join("PRD.md")).ok();
    let changeset_raw = std::fs::read_to_string(plan_dir.join("changeset.yaml")).ok();
    let prompt = evaluate::build_prompt(prd.as_deref(), changeset_raw.as_deref());
    let session_id = resolve_agent_session_id(plan_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", evaluate::system_prompt());
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    // Persist transitional state so resume (`next_goal_for_state` / run_workflow) does not keep
    // e.g. GreenComplete → next "demo" while the evaluate goal is actually running.
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "Evaluating");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_validate(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let eval_report = std::fs::read_to_string(plan_dir.join("evaluation-report.md"))
        .map_err(|e| format!("read evaluation-report.md: {}", e))?;
    let prompt = validate_subagents::build_prompt(&eval_report);
    let session_id = resolve_agent_session_id(plan_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", validate_subagents::system_prompt());
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "Validating");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_refactor(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let refactor_plan = std::fs::read_to_string(plan_dir.join("refactoring-plan.md"))
        .map_err(|e| format!("read refactoring-plan.md: {}", e))?;
    let prompt = refactor::build_prompt(&refactor_plan);
    let session_id = resolve_agent_session_id(plan_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", refactor::system_prompt());
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "Refactoring");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn before_update_docs(
    plan_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut artifacts = Vec::new();
    for (name, path) in [
        ("PRD.md", "PRD.md"),
        ("progress.md", "progress.md"),
        ("changeset.yaml", "changeset.yaml"),
        ("acceptance-tests.md", "acceptance-tests.md"),
        ("evaluation-report.md", "evaluation-report.md"),
        ("refactoring-plan.md", "refactoring-plan.md"),
    ] {
        if plan_dir.join(path).exists() {
            artifacts.push(format!("- {}: available", name));
        }
    }
    let artifacts_summary = if artifacts.is_empty() {
        "No artifacts found.".to_string()
    } else {
        artifacts.join("\n")
    };
    let prompt = update_docs::build_prompt(&artifacts_summary);
    context.set_sync("prompt", prompt);

    let mut system_prompt = update_docs::system_prompt();
    if let Ok(cs) = read_changeset(plan_dir) {
        if let Some(ref branch) = cs.branch {
            system_prompt.push_str("\n\n**FINAL STEP**: After completing all documentation updates, commit all modifications with a descriptive message and push to the remote branch: ");
            system_prompt.push_str(branch);
            system_prompt.push('.');
        }
    }
    context.set_sync("system_prompt", system_prompt);
    context.set_sync("plan_dir", plan_dir.to_path_buf());
    let session_id = resolve_agent_session_id(plan_dir)?;
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "UpdatingDocs");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn after_plan(plan_dir: &Path, context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let planning: PlanningOutput = context
        .get_sync("parsed_planning")
        .or_else(|| {
            let output: String = context.get_sync("output")?;
            parse_planning_response_with_base(&output, plan_dir).ok()
        })
        .ok_or("plan after_task requires parsed_planning or parseable output in context")?;
    write_artifacts(plan_dir, &planning)?;
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    let mut cs = read_changeset(plan_dir).unwrap_or_else(|_| Changeset::default());
    cs.name = planning.name.clone();
    cs.initial_prompt = Some(feature_input);
    cs.discovery = planning.discovery.clone();
    cs.branch_suggestion = planning.branch_suggestion.clone();
    cs.worktree_suggestion = planning.worktree_suggestion.clone();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    if session_exists {
        update_state(&mut cs, "Planned");
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            "plan",
            "Planned",
            &backend_name,
            Some("system-prompt-plan.md".to_string()),
        );
    }
    let _ = write_changeset(plan_dir, &cs);
    Ok(())
}

fn after_acceptance_tests(
    plan_dir: &Path,
    output: &str,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_acceptance_tests_response(output).map_err(WorkflowError::ParseError)?;
    write_acceptance_tests_file(plan_dir, &parsed)?;
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let mut cs = read_changeset(plan_dir).unwrap_or_default();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    if session_exists {
        update_state(&mut cs, "AcceptanceTestsReady");
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            "acceptance-tests",
            "AcceptanceTestsReady",
            &backend_name,
            None,
        );
    }
    let _ = write_changeset(plan_dir, &cs);
    Ok(())
}

fn after_red(
    plan_dir: &Path,
    output: &str,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_red_response(output).map_err(WorkflowError::ParseError)?;
    let _ = write_red_output_file(plan_dir, &parsed);
    let _ = write_progress_file(plan_dir, &parsed);
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let mut cs = read_changeset(plan_dir).unwrap_or_default();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    if session_exists {
        update_state(&mut cs, "RedTestsReady");
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            "impl",
            "RedTestsReady",
            &backend_name,
            None,
        );
    }
    let _ = write_changeset(plan_dir, &cs);
    Ok(())
}

fn after_green(plan_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_green_response(output).map_err(WorkflowError::ParseError)?;
    let _ = update_progress_file(plan_dir, &parsed);
    let _ = update_acceptance_tests_file(plan_dir, &parsed);
    if let Some(ref demo) = parsed.demo_results {
        let _ = write_demo_results_file(plan_dir, &demo.summary, demo.steps_completed);
    }
    if parsed.all_tests_passing() {
        if let Ok(mut cs) = read_changeset(plan_dir) {
            update_state(&mut cs, "GreenComplete");
            let _ = write_changeset(plan_dir, &cs);
        }
    }
    Ok(())
}

fn after_evaluate(plan_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_evaluate_response(output).map_err(WorkflowError::ParseError)?;
    let _ = write_evaluation_report(plan_dir, &parsed);
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "Evaluated");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn after_validate(plan_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_validate_subagents_response(output).map_err(WorkflowError::ParseError)?;
    let refactoring_plan_path = plan_dir.join("refactoring-plan.md");
    if let Some(plan_md) = parsed.refactoring_plan {
        std::fs::write(&refactoring_plan_path, plan_md).map_err(
            |e| -> Box<dyn Error + Send + Sync> {
                format!("write refactoring-plan.md: {}", e).into()
            },
        )?;
    } else if parsed.refactoring_plan_written && !refactoring_plan_path.exists() {
        let _ = std::fs::write(
            &refactoring_plan_path,
            "# Refactoring Plan\n## Tasks\n1. No-op refactoring task\n",
        );
    }
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "ValidateComplete");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn after_refactor(plan_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_refactor_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "RefactorComplete");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn after_update_docs(plan_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_update_docs_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "DocsUpdated");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

fn after_demo(plan_dir: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Ok(mut cs) = read_changeset(plan_dir) {
        update_state(&mut cs, "DemoComplete");
        let _ = write_changeset(plan_dir, &cs);
    }
    Ok(())
}

impl RunnerHooks for TddWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn progress_sink(&self, context: &Context) -> Option<ProgressSink> {
        let plan_dir: Option<PathBuf> = context
            .get_sync("plan_dir")
            .or_else(|| context.get_sync("output_dir"));
        let task_id: Option<String> = context.get_sync("current_task_id");
        let backend_name: String = context
            .get_sync("backend_name")
            .unwrap_or_else(|| "claude".to_string());
        let event_tx = self.event_tx.clone();

        Some(ProgressSink::new(move |ev: &StreamProgressEvent| {
            if let StreamProgressEvent::SessionStarted { session_id } = ev {
                if let Some(ref dir) = plan_dir {
                    if let Ok(mut cs) = read_changeset(dir) {
                        let already_exists = cs.sessions.iter().any(|s| s.id == *session_id);
                        if !already_exists {
                            let tag = match task_id.as_deref() {
                                Some("red") => "impl",
                                Some(t) => t,
                                None => "plan",
                            };
                            let now = chrono::Utc::now().to_rfc3339();
                            cs.sessions.push(SessionEntry {
                                id: session_id.clone(),
                                agent: backend_name.clone(),
                                tag: tag.to_string(),
                                created_at: now,
                                system_prompt_file: None,
                            });
                        }
                        cs.state.session_id = Some(session_id.clone());
                        let _ = write_changeset(dir, &cs);
                    }
                }
            }
            if let Some(ref tx) = event_tx {
                let _ = tx.send(WorkflowEvent::Progress(ev.clone()));
            }
        }))
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        context.set_sync("current_task_id", task_id.to_string());
        log::debug!("[tddy-core] state: → {}", task_id);
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        let plan_dir: Option<PathBuf> = context
            .get_sync("plan_dir")
            .or_else(|| context.get_sync("output_dir"));
        let plan_dir = match plan_dir {
            Some(p) => p,
            None => return Ok(()),
        };

        match task_id {
            "plan" => before_plan(&plan_dir, context)?,
            "acceptance-tests" => {
                ensure_worktree_for_acceptance_tests(&plan_dir, context, self.event_tx.as_ref())?;
                before_acceptance_tests(&plan_dir, context)?;
            }
            "red" => before_red(&plan_dir, context)?,
            "green" => before_green(&plan_dir, context)?,
            "demo" => before_demo(&plan_dir, context)?,
            "evaluate" => before_evaluate(&plan_dir, context)?,
            "validate" => before_validate(&plan_dir, context)?,
            "refactor" => before_refactor(&plan_dir, context)?,
            "update-docs" => before_update_docs(&plan_dir, context)?,
            _ => {}
        }
        // Emit transitional state (e.g. RedTesting, GreenImplementing) when starting a goal.
        // Skip when resuming from clarification (answers in context) to avoid duplicate emissions.
        let is_resuming = context.get_sync::<String>("answers").is_some();
        if !is_resuming {
            if let Some(ref tx) = self.event_tx {
                let from = read_changeset(&plan_dir)
                    .map(|c| c.state.current)
                    .unwrap_or_else(|_| "Init".to_string());
                let to_transitional = match task_id {
                    "plan" => Some("Planning"),
                    "acceptance-tests" => Some("AcceptanceTesting"),
                    "red" => Some("RedTesting"),
                    "green" => Some("GreenImplementing"),
                    "demo" => Some("DemoRunning"),
                    "evaluate" => Some("Evaluating"),
                    "validate" => Some("Validating"),
                    "refactor" => Some("Refactoring"),
                    "update-docs" => Some("UpdatingDocs"),
                    _ => None,
                };
                if let Some(to) = to_transitional {
                    let _ = tx.send(WorkflowEvent::StateChange {
                        from: from.clone(),
                        to: to.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let plan_dir: Option<PathBuf> = context
            .get_sync("plan_dir")
            .or_else(|| context.get_sync("output_dir"));
        if let (Some(ref tx), Some(ref dir)) = (&self.event_tx, plan_dir) {
            let current = read_changeset(dir)
                .map(|c| c.state.current)
                .unwrap_or_else(|_| "Init".to_string());
            let (from, to) = match task_id {
                "plan" => ("Planning", "Planned"),
                "acceptance-tests" => ("AcceptanceTesting", "AcceptanceTestsReady"),
                "red" => ("RedTesting", "RedTestsReady"),
                "green" => ("GreenImplementing", "GreenComplete"),
                "demo" => ("DemoRunning", "DemoComplete"),
                "evaluate" => ("Evaluating", "Evaluated"),
                "validate" => ("Validating", "ValidateComplete"),
                "refactor" => ("Refactoring", "RefactorComplete"),
                "update-docs" => ("UpdatingDocs", "DocsUpdated"),
                _ => (current.as_str(), current.as_str()),
            };
            if to != from {
                let _ = tx.send(WorkflowEvent::StateChange {
                    from: from.to_string(),
                    to: to.to_string(),
                });
                // Advance goal display so UI shows next phase (e.g. "Goal: red" when state is AcceptanceTestsReady)
                if let Some(next_goal) = next_goal_for_state(to) {
                    let _ = tx.send(WorkflowEvent::GoalStarted(next_goal.to_string()));
                }
            }
        }
        // Clear per-step resume flags so the next task starts fresh.
        context.remove_sync("answers");
        context.remove_sync("is_resume");
        match task_id {
            "plan" => {
                let plan_dir: PathBuf = context
                    .get_sync("plan_dir")
                    .ok_or("plan after_task requires plan_dir in context (set by PlanTask)")?;
                after_plan(&plan_dir, context)?;
            }
            "acceptance-tests" | "red" | "green" | "evaluate" => {
                let plan_dir: PathBuf = context
                    .get_sync("plan_dir")
                    .or_else(|| context.get_sync("output_dir"))
                    .ok_or("after_task requires plan_dir or output_dir in context")?;
                let output: String = context
                    .get_sync("output")
                    .ok_or("after_task requires output in context")?;
                match task_id {
                    "acceptance-tests" => after_acceptance_tests(&plan_dir, &output, context)?,
                    "red" => after_red(&plan_dir, &output, context)?,
                    "green" => after_green(&plan_dir, &output)?,
                    "evaluate" => after_evaluate(&plan_dir, &output)?,
                    _ => {}
                }
            }
            "validate" | "refactor" | "update-docs" | "demo" => {
                let output: Option<String> = context.get_sync("output");
                let plan_dir: Option<PathBuf> = context
                    .get_sync("plan_dir")
                    .or_else(|| context.get_sync("output_dir"));
                if let (Some(ref output), Some(ref plan_dir)) = (output, plan_dir) {
                    match task_id {
                        "validate" => after_validate(plan_dir, output)?,
                        "refactor" => after_refactor(plan_dir, output)?,
                        "update-docs" => after_update_docs(plan_dir, output)?,
                        "demo" => after_demo(plan_dir)?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn elicitation_after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Option<ElicitationEvent> {
        if task_id != "plan" {
            return None;
        }
        let plan_dir: PathBuf = context
            .get_sync("plan_dir")
            .or_else(|| context.get_sync("output_dir"))?;
        let prd_path = plan_dir.join("PRD.md");
        if !prd_path.exists() {
            return None;
        }
        let prd_content = std::fs::read_to_string(&prd_path).ok()?;
        Some(ElicitationEvent::PlanApproval { prd_content })
    }

    fn on_error(&self, _task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::error!("[tddy-core] workflow task failed: {}", error);
        let plan_dir: Option<PathBuf> = context
            .get_sync("plan_dir")
            .or_else(|| context.get_sync("output_dir"));
        let Some(ref dir) = plan_dir else {
            return;
        };
        let Ok(mut cs) = read_changeset(dir) else {
            return;
        };
        let from = cs.state.current.clone();
        update_state(&mut cs, "Failed");
        if write_changeset(dir, &cs).is_err() {
            return;
        }
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::StateChange {
                from,
                to: "Failed".to_string(),
            });
        }
    }
}
