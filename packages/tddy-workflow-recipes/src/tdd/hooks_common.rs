//! Shared helpers for [`super::hooks::TddWorkflowHooks`] and [`crate::tdd_small::hooks::TddSmallWorkflowHooks`]
//! to avoid behavioral drift between classic TDD and `tdd-small`.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tddy_core::changeset::{
    get_session_for_tag, read_changeset, resolve_model, update_state, write_changeset, Changeset,
};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::setup_worktree_for_session;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::find_git_root;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::tdd::green;
use crate::tdd::session_dir_resolve::resolve_existing_session_dir_for_plan;
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
    match setup_worktree_for_session(&repo_root, session_dir) {
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
