//! The `before_*` phase functions of the `tdd-small` workflow: each one prepares the context and
//! prompt for one goal, including the merged red phase. `impl RunnerHooks` in the parent `hooks.rs`
//! is their only caller.

use std::error::Error;
use std::path::{Path, PathBuf};

use tddy_core::changeset::{read_changeset, resolve_model, update_state};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::tdd::{hooks_common, refactor, update_docs};
use crate::tdd_small::{
    post_green_review,
    red::{build_merged_red_followup_prompt, build_merged_red_prompt, merged_red_system_prompt},
};
use crate::SessionArtifactManifest;

pub(crate) fn before_merged_red(
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

pub(crate) fn before_post_green_review(
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
    let session_id = hooks_common::resolve_agent_session_id(session_dir)?;
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("UpdatingDocs"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_update_docs UpdatingDocs");
    }
    Ok(())
}
