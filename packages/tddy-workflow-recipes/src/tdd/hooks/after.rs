//! The `after_*` phase functions of the full TDD workflow: each one parses a goal's structured
//! output and writes its artifacts. `impl RunnerHooks` in the parent `hooks.rs` is their only
//! caller.

use std::error::Error;
use std::path::Path;

use tddy_core::changeset::{
    append_session_and_update_state, read_changeset, update_state, BranchWorktreeIntent, Changeset,
};
use tddy_core::error::WorkflowError;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::TaskResult;

use crate::parser::{
    parse_acceptance_tests_response, parse_green_response, parse_planning_response_with_base,
    parse_red_response, parse_refactor_response, parse_update_docs_response, PlanningOutput,
};
use crate::tdd::{hooks_common, interview};
use crate::writer::write_artifacts;
use crate::{
    parse_evaluate_response, parse_validate_subagents_response, update_acceptance_tests_file,
    update_progress_file, write_acceptance_tests_file, write_demo_results_file,
    write_evaluation_report, write_progress_file, write_red_output_file, SessionArtifactManifest,
};

pub(crate) fn after_interview(
    session_dir: &Path,
    result: &TaskResult,
    handoff_snapshot: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::debug!(
        target: "tddy_workflow_recipes::tdd::hooks",
        "after_interview: persisting handoff for session_dir={:?}",
        session_dir
    );
    let mut text = handoff_snapshot.unwrap_or_default();
    if text.trim().is_empty() {
        text = result.response.clone();
    }
    interview::persist_interview_handoff_for_plan(session_dir, &text)?;
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Interviewed"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_interview Interviewed");
    }
    Ok(())
}

pub(crate) fn after_plan(
    _recipe: &dyn WorkflowRecipe,
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
        "[tdd hooks] after_plan writing session document basename={:?} under {:?}",
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
    let plan_session_tag = "plan";
    if session_exists {
        update_state(&mut cs, WorkflowState::new("Planned"));
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            plan_session_tag,
            WorkflowState::new("Planned"),
            &backend_name,
            Some("system-prompt-plan.md".to_string()),
        );
    }
    hooks_common::write_changeset_logged(session_dir, &cs, "after_plan Planned");
    Ok(())
}

pub(crate) fn after_acceptance_tests(
    session_dir: &Path,
    output: &str,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_acceptance_tests_response(output).map_err(WorkflowError::ParseError)?;
    write_acceptance_tests_file(session_dir, &parsed)?;
    let session_id: String = context
        .get_sync("session_id")
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());
    let mut cs = read_changeset(session_dir).unwrap_or_default();
    let session_exists = cs.sessions.iter().any(|s| s.id == session_id);
    if session_exists {
        update_state(&mut cs, WorkflowState::new("AcceptanceTestsReady"));
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            "acceptance-tests",
            WorkflowState::new("AcceptanceTestsReady"),
            &backend_name,
            None,
        );
    }
    hooks_common::write_changeset_logged(session_dir, &cs, "after_acceptance_tests");
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
    hooks_common::write_changeset_logged(session_dir, &cs, "after_red");
    Ok(())
}

pub(crate) fn after_green(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_green_response(output).map_err(WorkflowError::ParseError)?;
    let _ = update_progress_file(session_dir, &parsed);
    let _ = update_acceptance_tests_file(session_dir, &parsed);
    if let Some(ref demo) = parsed.demo_results {
        let _ = write_demo_results_file(session_dir, &demo.summary, demo.steps_completed);
    }
    if parsed.all_tests_passing() {
        if let Ok(mut cs) = read_changeset(session_dir) {
            update_state(&mut cs, WorkflowState::new("GreenComplete"));
            hooks_common::write_changeset_logged(session_dir, &cs, "after_green GreenComplete");
        }
    }
    Ok(())
}

pub(crate) fn after_evaluate(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_evaluate_response(output).map_err(WorkflowError::ParseError)?;
    let _ = write_evaluation_report(session_dir, &parsed);
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Evaluated"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_evaluate Evaluated");
    }
    Ok(())
}

pub(crate) fn after_validate(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = parse_validate_subagents_response(output).map_err(WorkflowError::ParseError)?;
    let refactoring_plan_path = session_dir.join("refactoring-plan.md");
    if let Some(plan_md) = parsed.refactoring_plan {
        tddy_core::atomic_file::write_atomic(&refactoring_plan_path, plan_md).map_err(
            |e| -> Box<dyn Error + Send + Sync> {
                format!("write refactoring-plan.md: {}", e).into()
            },
        )?;
    } else if !refactoring_plan_path.exists() {
        tddy_core::atomic_file::write_atomic(
            &refactoring_plan_path,
            "# Refactoring Plan\n## Tasks\n1. No-op refactoring task\n",
        )
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("write refactoring-plan.md fallback: {}", e).into()
        })?;
    }
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("ValidateComplete"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_validate ValidateComplete");
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

pub(crate) fn after_demo(session_dir: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("DemoComplete"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_demo DemoComplete");
    }
    Ok(())
}
