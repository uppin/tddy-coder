//! The `after_*` phase functions of the full TDD workflow: each one parses a goal's structured
//! output and writes its artifacts. `impl RunnerHooks` in the parent `hooks.rs` is their only
//! caller.

use std::error::Error;
use std::path::Path;

use tddy_core::changeset::{append_session_and_update_state, read_changeset, update_state};
use tddy_core::error::WorkflowError;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::TaskResult;

use crate::parser::parse_acceptance_tests_response;
use crate::tdd::{hooks_common, interview};
use crate::{
    parse_evaluate_response, parse_validate_subagents_response, write_acceptance_tests_file,
    write_demo_results_file, write_evaluation_report, SessionArtifactManifest,
};

pub(crate) fn after_plan(
    _recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    hooks_common::after_plan(
        manifest,
        session_dir,
        context,
        "tddy_workflow_recipes::tdd::hooks",
        "[tdd hooks]",
        "plan",
    )
}

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

pub(crate) fn after_green(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = hooks_common::parse_green_and_update_progress(session_dir, output)?;
    if let Some(ref demo) = parsed.demo_results {
        let _ = write_demo_results_file(session_dir, &demo.summary, demo.steps_completed);
    }
    hooks_common::complete_green_if_all_tests_passing(session_dir, &parsed);
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

pub(crate) fn after_demo(session_dir: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("DemoComplete"));
        hooks_common::write_changeset_logged(session_dir, &cs, "after_demo DemoComplete");
    }
    Ok(())
}
