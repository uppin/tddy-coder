//! The `after_*` phase functions of the `tdd-small` workflow: each one parses a goal's structured
//! output and writes its artifacts, including the single post-green review submit.
//! `impl RunnerHooks` in the parent `hooks.rs` is their only caller.

use std::error::Error;
use std::path::Path;

use tddy_core::changeset::{
    append_session_and_update_state, read_changeset, update_state, BranchWorktreeIntent, Changeset,
};
use tddy_core::error::WorkflowError;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::parser::{
    parse_green_response, parse_planning_response_with_base, parse_red_response,
    parse_refactor_response, parse_update_docs_response, PlanningOutput,
};
use crate::tdd::hooks_common;
use crate::tdd_small::parse_post_green_review_response;
use crate::writer::{
    update_acceptance_tests_file, update_progress_file, write_evaluation_report,
    write_progress_file, write_red_output_file,
};
use crate::{write_artifacts, EvaluateOutput, SessionArtifactManifest};

pub(crate) fn after_plan(
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
    if cs.workflow.as_ref().and_then(|w| w.branch_worktree_intent)
        == Some(BranchWorktreeIntent::NewBranchFromBase)
    {
        if let Some(ref b) = planning.branch_suggestion {
            if !b.trim().is_empty() {
                cs.workflow
                    .get_or_insert_with(Default::default)
                    .new_branch_name = Some(b.clone());
            }
        }
    }
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

pub(crate) fn after_red(
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

pub(crate) fn after_green(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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

pub(crate) fn after_post_green_review(
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
        tddy_core::atomic_file::write_atomic(
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

pub(crate) fn after_refactor(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = parse_refactor_response(output).map_err(WorkflowError::ParseError)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("RefactorComplete"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_refactor RefactorComplete");
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
        hooks_common::write_changeset_logged(session_dir, &cs, "after_update_docs DocsUpdated");
    }
    Ok(())
}
