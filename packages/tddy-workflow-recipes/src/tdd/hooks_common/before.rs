//! The `before_*` helpers both hook workflows share: each prepares context and prompt for one goal
//! that classic TDD and `tdd-small` run identically. The parent `hooks_common` re-exports them, so
//! every `hooks_common::before_*` call site resolves unchanged.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tddy_core::changeset::{read_changeset, resolve_model, update_state, Changeset};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::setup_worktree_for_session_with_optional_chain_base;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::find_git_root;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::recipe::WorkflowRecipe;

use super::{
    read_primary_session_document_optional, recipe_default_models_str, resolve_agent_session_id,
    write_changeset_logged,
};
use crate::tdd::session_dir_resolve::resolve_existing_session_dir_for_plan;
use crate::tdd::{green, refactor, update_docs};
use crate::SessionArtifactManifest;

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
