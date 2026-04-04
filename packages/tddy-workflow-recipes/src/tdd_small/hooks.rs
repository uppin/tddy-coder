//! Hooks for the `tdd-small` workflow: merged red, single post-green submit, shared green/refactor/docs.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use tddy_core::backend::{AgentOutputSink, ProgressSink};
use tddy_core::changeset::{
    append_session_and_update_state, read_changeset, resolve_model, update_state, write_changeset,
    Changeset, SessionEntry,
};
use tddy_core::error::WorkflowError;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::stream::ProgressEvent as StreamProgressEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::ElicitationEvent;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::TaskResult;

use crate::parser::{
    parse_green_response, parse_planning_response_with_base, parse_red_response,
    parse_refactor_response, parse_update_docs_response, EvaluateOutput, PlanningOutput,
};
use crate::tdd::hooks_common;
use crate::tdd::{refactor, update_docs};
use crate::tdd_small::parse_post_green_review_response;
use crate::tdd_small::post_green_review;
use crate::tdd_small::red::{
    build_merged_red_followup_prompt, build_merged_red_prompt, merged_red_system_prompt,
};
use crate::writer::{
    update_acceptance_tests_file, update_progress_file, write_artifacts, write_evaluation_report,
    write_progress_file, write_red_output_file,
};
use crate::SessionArtifactManifest;

fn before_merged_red(
    session_dir: &Path,
    context: &Context,
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!(
        "[tdd-small hooks] before_merged_red: session_dir={:?}",
        session_dir
    );
    let prd = hooks_common::read_primary_session_document(session_dir, manifest)?;
    let at = std::fs::read_to_string(session_dir.join("acceptance-tests.md")).unwrap_or_default();
    let changeset = read_changeset(session_dir).map_err(|e| e.to_string())?;
    let defaults = hooks_common::recipe_default_models_str(recipe);
    let model = resolve_model(
        Some(&changeset),
        "red",
        context.get_sync::<String>("model").as_deref(),
        Some(&defaults),
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => build_merged_red_followup_prompt(&prd, &at, a),
        None => build_merged_red_prompt(&prd, &at),
    };
    let repo_dir: Option<PathBuf> = context
        .get_sync("worktree_dir")
        .or_else(|| context.get_sync("output_dir"));
    let ctx_artifacts = manifest.context_header_filenames();
    let prompt = prepend_context_header(
        prompt,
        Some(session_dir),
        repo_dir.as_deref(),
        &ctx_artifacts,
    );
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", merged_red_system_prompt());
    let session_id = uuid::Uuid::now_v7().to_string();
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", false);
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("RedTesting"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_merged_red RedTesting");
    }
    Ok(())
}

fn before_post_green_review(
    session_dir: &Path,
    context: &Context,
    _recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!("[tdd-small hooks] before_post_green_review");
    let prd = hooks_common::read_primary_session_document_optional(session_dir, manifest);
    let changeset_raw = std::fs::read_to_string(session_dir.join("changeset.yaml")).ok();
    let prompt = post_green_review::build_prompt(prd.as_deref(), changeset_raw.as_deref());
    let session_id = hooks_common::resolve_agent_session_id(session_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", post_green_review::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Evaluating"));
        hooks_common::write_changeset_logged(
            session_dir,
            &cs,
            "before_post_green_review Evaluating",
        );
    }
    Ok(())
}

fn before_refactor(
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let refactor_plan = std::fs::read_to_string(session_dir.join("refactoring-plan.md"))
        .map_err(|e| format!("read refactoring-plan.md: {}", e))?;
    let prompt = refactor::build_prompt(&refactor_plan);
    let session_id = hooks_common::resolve_agent_session_id(session_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", refactor::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Refactoring"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_refactor Refactoring");
    }
    Ok(())
}

fn before_update_docs(
    manifest: &dyn SessionArtifactManifest,
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut artifacts = Vec::new();
    for (key, filename) in manifest.known_artifacts() {
        let available = if *key == "prd" {
            manifest
                .primary_document_basename()
                .map(|bn| {
                    tddy_workflow::resolve_existing_session_artifact(session_dir, &bn).is_some()
                })
                .unwrap_or(false)
        } else {
            session_dir.join(filename).exists()
                || session_dir.join("artifacts").join(filename).exists()
        };
        if available {
            artifacts.push(format!("- {}: available", filename));
        }
    }
    if session_dir.join("changeset.yaml").exists() {
        artifacts.push("- changeset.yaml: available".to_string());
    }
    let artifacts_summary = if artifacts.is_empty() {
        "No artifacts found.".to_string()
    } else {
        artifacts.join("\n")
    };
    let prompt = update_docs::build_prompt(&artifacts_summary);
    context.set_sync("prompt", prompt);

    let mut system_prompt = update_docs::system_prompt();
    if let Ok(cs) = read_changeset(session_dir) {
        if let Some(ref branch) = cs.branch {
            system_prompt.push_str("\n\n**FINAL STEP**: After completing all documentation updates, commit all modifications with a descriptive message and push to the remote branch: ");
            system_prompt.push_str(branch);
            system_prompt.push('.');
        }
    }
    context.set_sync("system_prompt", system_prompt);
    context.set_sync("session_dir", session_dir.to_path_buf());
    let session_id = hooks_common::resolve_agent_session_id(session_dir)?;
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("UpdatingDocs"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_update_docs UpdatingDocs");
    }
    Ok(())
}

fn after_plan(
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let planning: PlanningOutput = context
        .get_sync("parsed_planning")
        .or_else(|| {
            let output: String = context.get_sync("output")?;
            parse_planning_response_with_base(&output, session_dir).ok()
        })
        .ok_or("plan after_task requires parsed_planning or parseable output in context")?;
    let prd_bn = manifest
        .primary_document_basename()
        .ok_or("plan after_task requires primary session document basename (prd) in manifest")?;
    log::info!(
        "[tdd-small hooks] after_plan writing session document basename={:?} under {:?}",
        prd_bn,
        session_dir
    );
    write_artifacts(session_dir, &planning, &prd_bn)?;
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    let mut cs = read_changeset(session_dir).unwrap_or_else(|_| Changeset::default());
    cs.name = planning.name.clone();
    cs.initial_prompt = Some(feature_input);
    cs.discovery = planning.discovery.clone();
    cs.branch_suggestion = planning.branch_suggestion.clone();
    cs.worktree_suggestion = planning.worktree_suggestion.clone();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    let start_tag = recipe.start_goal().as_str().to_string();
    if session_exists {
        update_state(&mut cs, WorkflowState::new("Planned"));
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            &start_tag,
            WorkflowState::new("Planned"),
            &backend_name,
            Some("system-prompt-plan.md".to_string()),
        );
    }
    hooks_common::write_changeset_logged(session_dir, &cs, "after_plan Planned");
    Ok(())
}

fn after_red(
    session_dir: &Path,
    output: &str,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_red_response(output).map_err(WorkflowError::ParseError)?;
    let _ = write_red_output_file(session_dir, &parsed);
    let _ = write_progress_file(session_dir, &parsed);
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let mut cs = read_changeset(session_dir).unwrap_or_default();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    if session_exists {
        update_state(&mut cs, WorkflowState::new("RedTestsReady"));
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            "impl",
            WorkflowState::new("RedTestsReady"),
            &backend_name,
            None,
        );
    }
    hooks_common::write_changeset_logged(session_dir, &cs, "after_red RedTestsReady");
    Ok(())
}

fn after_green(session_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_green_response(output).map_err(WorkflowError::ParseError)?;
    let _ = update_progress_file(session_dir, &parsed);
    let _ = update_acceptance_tests_file(session_dir, &parsed);
    if parsed.all_tests_passing() {
        if let Ok(mut cs) = read_changeset(session_dir) {
            update_state(&mut cs, WorkflowState::new("GreenComplete"));
            hooks_common::write_changeset_logged(session_dir, &cs, "after_green GreenComplete");
        }
    }
    Ok(())
}

fn after_post_green_review(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!(
        "[tdd-small hooks] after_post_green_review: persisting merged evaluate+validate artifacts"
    );
    let parsed = parse_post_green_review_response(output).map_err(WorkflowError::ParseError)?;
    let eval = EvaluateOutput {
        summary: parsed.summary.clone(),
        risk_level: parsed.risk_level.clone(),
        build_results: vec![],
        issues: vec![],
        changeset_sync: None,
        files_analyzed: vec![],
        test_impact: None,
        changed_files: vec![],
        affected_tests: vec![],
        validity_assessment: parsed.validity_assessment.clone(),
    };
    write_evaluation_report(session_dir, &eval)?;
    let refactoring_plan_path = session_dir.join("refactoring-plan.md");
    if !refactoring_plan_path.exists() {
        std::fs::write(
            &refactoring_plan_path,
            format!(
                "# Refactoring plan (tdd-small post-green)\n\n\
                 Summary: {}\n\n\
                 Report flags — tests: {} prod_ready: {} clean_code: {}\n",
                parsed.summary,
                parsed.tests_report_written,
                parsed.prod_ready_report_written,
                parsed.clean_code_report_written
            ),
        )
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("write refactoring-plan: {}", e).into()
        })?;
    }
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("ValidateComplete"));
        hooks_common::write_changeset_logged(
            session_dir,
            &cs,
            "after_post_green_review ValidateComplete",
        );
    }
    Ok(())
}

fn after_refactor(session_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_refactor_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("RefactorComplete"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_refactor RefactorComplete");
    }
    Ok(())
}

fn after_update_docs(session_dir: &Path, output: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_update_docs_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("DocsUpdated"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_update_docs DocsUpdated");
    }
    Ok(())
}

/// Hooks for the `tdd-small` workflow.
pub struct TddSmallWorkflowHooks {
    recipe: Arc<dyn WorkflowRecipe>,
    manifest: Arc<dyn SessionArtifactManifest>,
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl TddSmallWorkflowHooks {
    /// CLI path: file I/O only (no events).
    pub fn new(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
    ) -> Self {
        Self {
            recipe,
            manifest,
            event_tx: None,
        }
    }

    /// Hooks with optional TUI event channel.
    pub fn with_event_tx_optional(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
        event_tx: Option<tddy_core::workflow::recipe::WorkflowEventSender>,
    ) -> Self {
        Self {
            recipe,
            manifest,
            event_tx,
        }
    }
}

impl RunnerHooks for TddSmallWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn progress_sink(&self, context: &Context) -> Option<ProgressSink> {
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        let task_id: Option<String> = context.get_sync("current_task_id");
        let backend_name: String = context
            .get_sync("backend_name")
            .unwrap_or_else(|| "claude".to_string());
        let event_tx = self.event_tx.clone();

        let recipe_for_progress = self.recipe.clone();
        Some(ProgressSink::new(move |ev: &StreamProgressEvent| {
            if let StreamProgressEvent::SessionStarted { session_id } = ev {
                if let Some(ref dir) = session_dir {
                    if let Ok(mut cs) = read_changeset(dir) {
                        let already_exists = cs.sessions.iter().any(|s| s.id == *session_id);
                        if !already_exists {
                            let tag = match task_id.as_deref() {
                                Some("red") => "impl".to_string(),
                                Some(t) => t.to_string(),
                                None => recipe_for_progress.start_goal().to_string(),
                            };
                            let now = chrono::Utc::now().to_rfc3339();
                            cs.sessions.push(SessionEntry {
                                id: session_id.clone(),
                                agent: backend_name.clone(),
                                tag,
                                created_at: now,
                                system_prompt_file: None,
                            });
                        }
                        cs.state.session_id = Some(session_id.clone());
                        hooks_common::write_changeset_logged(
                            dir,
                            &cs,
                            "tdd_small progress_sink SessionStarted",
                        );
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
        log::debug!("[tdd-small hooks] before_task task_id={}", task_id);
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        if task_id == "plan" {
            hooks_common::before_plan(context)?;
        } else {
            let session_dir: Option<PathBuf> = context
                .get_sync("session_dir")
                .or_else(|| context.get_sync("output_dir"));
            let session_dir = match session_dir {
                Some(p) => p,
                None => return Ok(()),
            };

            match task_id {
                "red" => {
                    hooks_common::ensure_worktree_for_session(
                        &session_dir,
                        context,
                        self.event_tx.as_ref(),
                        "[tdd-small hooks] merged red",
                    )?;
                    before_merged_red(
                        &session_dir,
                        context,
                        self.recipe.as_ref(),
                        self.manifest.as_ref(),
                    )?;
                }
                "green" => hooks_common::before_green(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                    "tddy_workflow_recipes::tdd_small::hooks",
                )?,
                "post-green-review" => before_post_green_review(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                )?,
                "refactor" => before_refactor(&session_dir, context)?,
                "update-docs" => before_update_docs(self.manifest.as_ref(), &session_dir, context)?,
                _ => {}
            }
        }
        let is_resuming = context.get_sync::<String>("answers").is_some();
        if !is_resuming {
            if let Some(ref tx) = self.event_tx {
                let session_dir_for_state: Option<PathBuf> = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"));
                let from = session_dir_for_state
                    .as_ref()
                    .and_then(|sd| read_changeset(sd).ok())
                    .map(|c| c.state.current)
                    .unwrap_or_else(|| WorkflowState::new("Init"));
                let to_transitional = match task_id {
                    "plan" => Some("Planning"),
                    "red" => Some("RedTesting"),
                    "green" => Some("GreenImplementing"),
                    "post-green-review" => Some("Evaluating"),
                    "refactor" => Some("Refactoring"),
                    "update-docs" => Some("UpdatingDocs"),
                    _ => None,
                };
                if let Some(to) = to_transitional {
                    let _ = tx.send(WorkflowEvent::StateChange {
                        from: from.to_string(),
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
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        if let (Some(ref tx), Some(ref dir)) = (&self.event_tx, session_dir) {
            let current = read_changeset(dir)
                .ok()
                .map(|c| c.state.current)
                .unwrap_or_else(|| WorkflowState::new("Init"));
            let (from, to) = match task_id {
                "plan" => ("Planning", "Planned"),
                "red" => ("RedTesting", "RedTestsReady"),
                "green" => ("GreenImplementing", "GreenComplete"),
                "post-green-review" => ("Evaluating", "ValidateComplete"),
                "refactor" => ("Refactoring", "RefactorComplete"),
                "update-docs" => ("UpdatingDocs", "DocsUpdated"),
                _ => (current.as_str(), current.as_str()),
            };
            if to != from {
                let _ = tx.send(WorkflowEvent::StateChange {
                    from: from.to_string(),
                    to: to.to_string(),
                });
                if let Some(next_goal) = self.recipe.next_goal_for_state(&WorkflowState::new(to)) {
                    let _ = tx.send(WorkflowEvent::GoalStarted(next_goal.to_string()));
                }
            }
        }
        context.remove_sync("answers");
        context.remove_sync("is_resume");
        match task_id {
            "plan" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .ok_or("plan after_task requires session_dir in context (set by PlanTask)")?;
                after_plan(
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                    &session_dir,
                    context,
                )?;
            }
            "red" | "green" | "post-green-review" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"))
                    .ok_or("after_task requires session_dir or output_dir in context")?;
                let output: String = context
                    .get_sync("output")
                    .ok_or("after_task requires output in context")?;
                match task_id {
                    "red" => after_red(&session_dir, &output, context)?,
                    "green" => after_green(&session_dir, &output)?,
                    "post-green-review" => after_post_green_review(&session_dir, &output)?,
                    _ => {}
                }
            }
            "refactor" | "update-docs" => {
                let output: Option<String> = context.get_sync("output");
                let session_dir: Option<PathBuf> = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"));
                if let (Some(ref output), Some(ref session_dir)) = (output, session_dir) {
                    match task_id {
                        "refactor" => after_refactor(session_dir, output)?,
                        "update-docs" => after_update_docs(session_dir, output)?,
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
        if task_id != self.recipe.start_goal().as_str() {
            return None;
        }
        let session_dir: PathBuf = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"))?;
        let basename = self.manifest.primary_document_basename()?;
        let prd_path = tddy_workflow::resolve_existing_session_artifact(&session_dir, &basename)?;
        log::debug!(
            "[tdd-small hooks] elicitation DocumentApproval reading {:?}",
            prd_path
        );
        let prd_content = std::fs::read_to_string(&prd_path).ok()?;
        Some(ElicitationEvent::DocumentApproval {
            content: prd_content,
        })
    }

    fn on_error(&self, _task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::error!("[tdd-small hooks] workflow task failed: {}", error);
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        let Some(ref dir) = session_dir else {
            return;
        };
        let Ok(mut cs) = read_changeset(dir) else {
            return;
        };
        let from = cs.state.current.to_string();
        update_state(&mut cs, WorkflowState::new("Failed"));
        if let Err(e) = write_changeset(dir, &cs) {
            log::warn!(
                "[tdd-small hooks] on_error: could not persist Failed state: {} (session_dir={})",
                e,
                dir.display()
            );
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
