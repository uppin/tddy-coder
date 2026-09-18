//! Shared helpers for [`super::hooks::TddWorkflowHooks`] and [`crate::tdd_small::hooks::TddSmallWorkflowHooks`]
//! to avoid behavioral drift between classic TDD and `tdd-small`.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use tddy_core::backend::{AgentOutputSink, ProgressSink};
use tddy_core::changeset::{
    append_session_and_update_state, get_session_for_tag, read_changeset, resolve_model,
    update_state, write_changeset, Changeset, SessionEntry,
};
use tddy_core::error::WorkflowError;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::setup_worktree_for_session_with_optional_chain_base;
use tddy_core::stream::ProgressEvent as StreamProgressEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::find_git_root;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::parser::{
    parse_green_response, parse_red_response, parse_refactor_response, parse_update_docs_response,
    GreenOutput,
};
use crate::tdd::session_dir_resolve::resolve_existing_session_dir_for_plan;
use crate::tdd::{green, refactor, update_docs};
use crate::writer::{
    update_acceptance_tests_file, update_progress_file, write_progress_file, write_red_output_file,
};
use crate::SessionArtifactManifest;

/// Read primary planning document using recipe basename and migration-aware resolution.
pub(crate) fn read_primary_session_document(
    session_dir: &Path,
    manifest: &dyn SessionArtifactManifest,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let bn = manifest
        .primary_document_basename()
        .ok_or("recipe has no primary session document key (prd) in manifest")?;
    let path =
        tddy_workflow::resolve_existing_session_artifact(session_dir, &bn).ok_or_else(|| {
            format!(
                "primary planning document ({}) not found under {:?}",
                bn, session_dir
            )
        })?;
    log::debug!(
        target: "tddy_workflow_recipes::tdd::hooks_common",
        "read_primary_session_document: {:?}",
        path
    );
    std::fs::read_to_string(&path)
        .map_err(|e| format!("read primary planning document {}: {}", path.display(), e).into())
}

pub(crate) fn read_primary_session_document_optional(
    session_dir: &Path,
    manifest: &dyn SessionArtifactManifest,
) -> Option<String> {
    let bn = manifest.primary_document_basename()?;
    tddy_workflow::read_session_artifact_utf8(session_dir, &bn)
}

pub(crate) fn recipe_default_models_str(recipe: &dyn WorkflowRecipe) -> BTreeMap<String, String> {
    recipe
        .default_models()
        .into_iter()
        .map(|(k, v)| (k.as_str().to_string(), v))
        .collect()
}

pub(crate) fn resolve_agent_session_id(
    session_dir: &Path,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let changeset =
        read_changeset(session_dir).map_err(|e| -> Box<dyn Error + Send + Sync> { Box::new(e) })?;
    changeset
        .state
        .session_id
        .clone()
        .or_else(|| get_session_for_tag(&changeset, "impl"))
        .or_else(|| {
            std::fs::read_to_string(session_dir.join(".impl-session"))
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

/// Persist `changeset.yaml`; on failure log a warning (hooks historically ignored errors to avoid breaking the TUI turn).
pub(crate) fn write_changeset_logged(session_dir: &Path, cs: &Changeset, operation: &'static str) {
    if let Err(e) = write_changeset(session_dir, cs) {
        log::warn!(
            target: "tddy_workflow_recipes::tdd::hooks_common",
            "write_changeset failed during {}: {} (session_dir={})",
            operation,
            e,
            session_dir.display()
        );
    }
}

pub(crate) fn before_plan(context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    let dir: PathBuf = resolve_existing_session_dir_for_plan(context).map_err(|e| {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
            as Box<dyn Error + Send + Sync>
    })?;
    context.set_sync("session_dir", dir.clone());
    if read_changeset(&dir).is_err() {
        let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
        let repo_path = context
            .get_sync::<PathBuf>("output_dir")
            .map(|p| p.display().to_string());
        let init_cs = Changeset {
            initial_prompt: Some(feature_input),
            repo_path,
            ..Changeset::default()
        };
        write_changeset_logged(&dir, &init_cs, "before_plan init changeset");
    }
    let mut cs = read_changeset(&dir).map_err(|e| e.to_string())?;
    update_state(&mut cs, WorkflowState::new("Planning"));
    write_changeset_logged(&dir, &cs, "before_plan Planning state");
    Ok(())
}

/// Ensure worktree exists (shared by classic `acceptance-tests` and `tdd-small` merged `red`).
/// `log_label` prefixes stub-backend and error logs for traceability.
pub(crate) fn ensure_worktree_for_session(
    session_dir: &Path,
    context: &Context,
    event_tx: Option<&mpsc::Sender<WorkflowEvent>>,
    log_label: &'static str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if context.get_sync::<PathBuf>("worktree_dir").is_some() {
        return Ok(());
    }
    let cs = read_changeset(session_dir).map_err(|e| e.to_string())?;
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
            "{}: stub backend, using output_dir as worktree (no git fetch)",
            log_label
        );
        context.set_sync("worktree_dir", output_dir.clone());
        if let Some(tx) = event_tx {
            let _ = tx.send(WorkflowEvent::WorktreeSwitched { path: output_dir });
        }
        return Ok(());
    }

    let repo_root = find_git_root(&output_dir);
    // No stack_parent in the workflow-recipe path: the chain base is the operator-selected
    // persisted field (or the default base when unset). Routing through
    // resolve_chain_base_for_session_spawn would require a real sessions_base (two levels above
    // session_dir); the direct read is correct and avoids a misleading unused argument.
    let chain_base = cs.worktree_integration_base_ref.as_deref();
    match setup_worktree_for_session_with_optional_chain_base(&repo_root, session_dir, chain_base) {
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
                "{}: worktree creation failed: repo_root={:?}, session_dir={:?}, error={}",
                log_label,
                repo_root,
                session_dir,
                e
            );
            Err(format!("worktree creation failed: {}", e).into())
        }
    }
}

pub(crate) fn before_green(
    session_dir: &Path,
    context: &Context,
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
    log_target: &'static str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let progress = std::fs::read_to_string(session_dir.join("progress.md"))
        .map_err(|e| format!("read progress.md: {}", e))?;
    let prd = read_primary_session_document_optional(session_dir, manifest);
    let at = std::fs::read_to_string(session_dir.join("acceptance-tests.md")).ok();
    let changeset = read_changeset(session_dir).ok();
    let new_agent_session = context
        .get_sync::<bool>("new_agent_session")
        .unwrap_or(false);
    if new_agent_session {
        context.remove_sync("session_id");
        context.set_sync("is_resume", false);
    } else {
        let session_id = resolve_agent_session_id(session_dir).map_err(|e| {
            format!(
                "green requires changeset with state.session_id, impl session, or .impl-session file: {}",
                e
            )
        })?;
        context.set_sync("session_id", session_id);
        context.set_sync("is_resume", true);
    }
    let defaults = recipe_default_models_str(recipe);
    let model = resolve_model(
        changeset.as_ref(),
        "green",
        context.get_sync::<String>("model").as_deref(),
        Some(&defaults),
    );
    let run_optional_step_x = context
        .get_sync::<bool>("run_optional_step_x")
        .unwrap_or(false);
    log::debug!(
        target: log_target,
        "before_green: run_optional_step_x={} new_agent_session={}",
        run_optional_step_x,
        new_agent_session
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => green::build_followup_prompt(&progress, a, prd.as_deref(), at.as_deref()),
        None => green::build_prompt(&progress, prd.as_deref(), at.as_deref()),
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
    context.set_sync("system_prompt", green::system_prompt(run_optional_step_x));
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("GreenImplementing"));
        write_changeset_logged(session_dir, &cs, "before_green GreenImplementing");
    }
    Ok(())
}

pub(crate) fn before_refactor(
    session_dir: &Path,
    context: &Context,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let refactor_plan = std::fs::read_to_string(session_dir.join("refactoring-plan.md"))
        .map_err(|e| format!("read refactoring-plan.md: {}", e))?;
    let prompt = refactor::build_prompt(&refactor_plan);
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
    let session_id = resolve_agent_session_id(session_dir)?;
    context.set_sync("prompt", prompt);
    context.set_sync("system_prompt", refactor::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Refactoring"));
        write_changeset_logged(session_dir, &cs, "before_refactor Refactoring");
    }
    Ok(())
}

pub(crate) fn before_update_docs(
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
    let session_id = resolve_agent_session_id(session_dir)?;
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("UpdatingDocs"));
        write_changeset_logged(session_dir, &cs, "before_update_docs UpdatingDocs");
    }
    Ok(())
}

pub(crate) fn after_refactor(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_refactor_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("RefactorComplete"));
        write_changeset_logged(session_dir, &cs, "after_refactor RefactorComplete");
    }
    Ok(())
}

pub(crate) fn after_update_docs(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_update_docs_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("DocsUpdated"));
        write_changeset_logged(session_dir, &cs, "after_update_docs DocsUpdated");
    }
    Ok(())
}

/// The agent-output sink both workflows install in `on_enter_task`.
pub(crate) fn agent_output_sink(
    event_tx: Option<&mpsc::Sender<WorkflowEvent>>,
) -> Option<AgentOutputSink> {
    event_tx.map(|tx| {
        let tx = tx.clone();
        AgentOutputSink::new(move |s: &str| {
            let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
        })
    })
}

/// `changeset_operation` carries the only difference between the two workflows: the operation each
/// one records on its `RedTestsReady` changeset write.
pub(crate) fn after_red(
    session_dir: &Path,
    output: &str,
    context: &Context,
    changeset_operation: &'static str,
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
    write_changeset_logged(session_dir, &cs, changeset_operation);
    Ok(())
}

/// The progress sink both workflows install in `on_enter_task`. `changeset_operation` carries the
/// only difference: the operation each one records on the `SessionStarted` write.
pub(crate) fn progress_sink(
    context: &Context,
    recipe: Arc<dyn WorkflowRecipe>,
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
    changeset_operation: &'static str,
) -> Option<ProgressSink> {
    let session_dir: Option<PathBuf> = context
        .get_sync("session_dir")
        .or_else(|| context.get_sync("output_dir"));
    let task_id: Option<String> = context.get_sync("current_task_id");
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());

    Some(ProgressSink::new(move |ev: &StreamProgressEvent| {
        if let StreamProgressEvent::SessionStarted { session_id } = ev {
            if let Some(ref dir) = session_dir {
                if let Ok(mut cs) = read_changeset(dir) {
                    let already_exists = cs.sessions.iter().any(|s| s.id == *session_id);
                    if !already_exists {
                        let tag = match task_id.as_deref() {
                            Some("red") => "impl".to_string(),
                            Some(t) => t.to_string(),
                            None => recipe.start_goal().to_string(),
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
                    write_changeset_logged(dir, &cs, changeset_operation);
                }
            }
        }
        if let Some(ref tx) = event_tx {
            let _ = tx.send(WorkflowEvent::Progress(ev.clone()));
        }
    }))
}

/// The parse-and-persist opening of `after_green`, shared by both workflows. Returns the parsed
/// output so each caller writes its own remaining artifacts in its own order.
pub(crate) fn parse_green_and_update_progress(
    session_dir: &Path,
    output: &str,
) -> Result<GreenOutput, Box<dyn Error + Send + Sync>> {
    let parsed = parse_green_response(output).map_err(WorkflowError::ParseError)?;
    let _ = update_progress_file(session_dir, &parsed);
    let _ = update_acceptance_tests_file(session_dir, &parsed);
    Ok(parsed)
}

/// The `GreenComplete` transition both workflows make once every test passes — the one piece of
/// `after_green` whose silent drift `hooks_common` exists to prevent.
pub(crate) fn complete_green_if_all_tests_passing(session_dir: &Path, parsed: &GreenOutput) {
    if parsed.all_tests_passing() {
        if let Ok(mut cs) = read_changeset(session_dir) {
            update_state(&mut cs, WorkflowState::new("GreenComplete"));
            write_changeset_logged(session_dir, &cs, "after_green GreenComplete");
        }
    }
}
