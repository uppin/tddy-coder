//! The `before_*` phase functions of the full TDD workflow: each one prepares the context and
//! prompt for one goal before its agent runs. `impl RunnerHooks` in the parent `hooks.rs` is their
//! only caller.

use std::error::Error;
use std::path::{Path, PathBuf};

use tddy_core::changeset::{read_changeset, resolve_model, update_state};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::prepend_context_header;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::tdd::{
    acceptance_tests, demo, evaluate, hooks_common, interview, red, validate_subagents,
};
use crate::SessionArtifactManifest;

pub(crate) fn before_interview(context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::debug!(
        target: "tddy_workflow_recipes::tdd::hooks",
        "before_interview: preparing prompts and session_dir"
    );
    let session_dir: PathBuf = context
        .get_sync("session_dir")
        .or_else(|| context.get_sync("output_dir"))
        .ok_or("interview requires session_dir or output_dir")?;
    context.set_sync("session_dir", session_dir.clone());

    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    if let Some(answers) = context.get_sync::<String>("answers") {
        if !answers.trim().is_empty() {
            context.set_sync(
                "prompt",
                interview::build_followup_prompt(&feature_input, &answers),
            );
            context.remove_sync("answers");
        } else {
            context.set_sync(
                "prompt",
                interview::build_interview_user_prompt(&feature_input),
            );
        }
    } else {
        context.set_sync(
            "prompt",
            interview::build_interview_user_prompt(&feature_input),
        );
    }
    context.set_sync("system_prompt", interview::system_prompt());
    if context
        .get_sync::<String>("session_id")
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        let session_id = uuid::Uuid::now_v7().to_string();
        log::debug!(
            target: "tddy_workflow_recipes::tdd::hooks",
            "before_interview: allocating new session_id (none in context)"
        );
        context.set_sync("session_id", session_id);
    } else {
        log::debug!(
            target: "tddy_workflow_recipes::tdd::hooks",
            "before_interview: keeping existing session_id (workflow session)"
        );
    }
    context.set_sync("is_resume", false);
    let model = context
        .get_sync::<String>("model")
        .unwrap_or_else(|| "sonnet".to_string());
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(&session_dir) {
        update_state(&mut cs, WorkflowState::new("Interviewing"));
        hooks_common::write_changeset_logged(&session_dir, &cs, "before_interview Interviewing");
    }
    Ok(())
}

pub(crate) fn before_plan_with_interview(
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    hooks_common::before_plan(context)?;
    let session_dir: PathBuf = context
        .get_sync("session_dir")
        .ok_or("before_plan_with_interview requires session_dir")?;
    interview::apply_staged_interview_handoff_to_plan_context(&session_dir, context)?;
    Ok(())
}

pub(crate) fn before_acceptance_tests(
    session_dir: &Path,
    context: &Context,
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = hooks_common::read_primary_session_document(session_dir, manifest)?;
    let changeset = read_changeset(session_dir).map_err(|e| e.to_string())?;
    let defaults = hooks_common::recipe_default_models_str(recipe);
    let model = resolve_model(
        Some(&changeset),
        "acceptance-tests",
        context.get_sync::<String>("model").as_deref(),
        Some(&defaults),
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => acceptance_tests::build_followup_prompt(&prd, a),
        None => acceptance_tests::build_prompt(&prd),
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
    context.set_sync("system_prompt", acceptance_tests::system_prompt());
    if context
        .get_sync::<String>("session_id")
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        let session_id = uuid::Uuid::now_v7().to_string();
        log::debug!(
            target: "tddy_workflow_recipes::tdd::hooks",
            "before_acceptance_tests: allocating new session_id (none in context)"
        );
        context.set_sync("session_id", session_id);
    }
    context.set_sync("is_resume", false);
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("model", model);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("AcceptanceTesting"));
        hooks_common::write_changeset_logged(
            session_dir,
            &cs,
            "before_acceptance_tests AcceptanceTesting",
        );
    }
    Ok(())
}

pub(crate) fn before_red(
    session_dir: &Path,
    context: &Context,
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = hooks_common::read_primary_session_document(session_dir, manifest)?;
    let at = std::fs::read_to_string(session_dir.join("acceptance-tests.md"))
        .map_err(|e| format!("read acceptance-tests.md: {}", e))?;
    let changeset = read_changeset(session_dir).ok();
    let defaults = hooks_common::recipe_default_models_str(recipe);
    let model = resolve_model(
        changeset.as_ref(),
        "red",
        context.get_sync::<String>("model").as_deref(),
        Some(&defaults),
    );
    let answers: Option<String> = context.get_sync("answers");
    let prompt = match &answers {
        Some(a) => red::build_followup_prompt(&prd, &at, a),
        None => red::build_prompt(&prd, &at),
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
    context.set_sync("system_prompt", red::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("model", model);
    if context
        .get_sync::<String>("session_id")
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        let session_id = uuid::Uuid::now_v7().to_string();
        log::debug!(
            target: "tddy_workflow_recipes::tdd::hooks",
            "before_red: allocating new session_id (none in context)"
        );
        context.set_sync("session_id", session_id);
    }
    context.set_sync("is_resume", false);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("RedTesting"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_red RedTesting");
    }
    Ok(())
}

pub(crate) fn before_demo(
    session_dir: &Path,
    context: &Context,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let demo_plan = std::fs::read_to_string(session_dir.join("demo-plan.md"))
        .map_err(|e| format!("read demo-plan.md: {}", e))?;
    let prompt = format!(
        "Execute the demo described in demo-plan.md:\n\n{}",
        demo_plan
    );
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
    context.set_sync("system_prompt", demo::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("DemoRunning"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_demo DemoRunning");
    }
    Ok(())
}

pub(crate) fn before_evaluate(
    session_dir: &Path,
    context: &Context,
    _recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prd = hooks_common::read_primary_session_document_optional(session_dir, manifest);
    let changeset_raw = std::fs::read_to_string(session_dir.join("changeset.yaml")).ok();
    let prompt = evaluate::build_prompt(prd.as_deref(), changeset_raw.as_deref());
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
    context.set_sync("system_prompt", evaluate::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Evaluating"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_evaluate Evaluating");
    }
    Ok(())
}

pub(crate) fn before_validate(
    session_dir: &Path,
    context: &Context,
    manifest: &dyn SessionArtifactManifest,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let eval_report = std::fs::read_to_string(session_dir.join("evaluation-report.md"))
        .map_err(|e| format!("read evaluation-report.md: {}", e))?;
    let prompt = validate_subagents::build_prompt(&eval_report);
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
    context.set_sync("system_prompt", validate_subagents::system_prompt());
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("session_id", session_id);
    context.set_sync("is_resume", true);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Validating"));
        hooks_common::write_changeset_logged(session_dir, &cs, "before_validate Validating");
    }
    Ok(())
}
