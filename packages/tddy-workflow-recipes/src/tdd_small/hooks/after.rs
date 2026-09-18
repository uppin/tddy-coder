//! The `after_*` phase functions of the `tdd-small` workflow: each one parses a goal's structured
//! output and writes its artifacts, including the single post-green review submit.
//! `impl RunnerHooks` in the parent `hooks.rs` is their only caller.

use std::error::Error;
use std::path::Path;

use tddy_core::changeset::{read_changeset, update_state};
use tddy_core::error::WorkflowError;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::tdd::hooks_common;
use crate::tdd_small::parse_post_green_review_response;
use crate::writer::write_evaluation_report;
use crate::{EvaluateOutput, SessionArtifactManifest};

pub(crate) fn after_plan(
    recipe: &dyn WorkflowRecipe,
    manifest: &dyn SessionArtifactManifest,
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    hooks_common::after_plan(
        manifest,
        session_dir,
        context,
        "tddy_workflow_recipes::tdd_small::hooks",
        "[tdd-small hooks]",
        recipe.start_goal().as_str(),
    )
}

pub(crate) fn after_green(
    session_dir: &Path,
    output: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parsed = hooks_common::parse_green_and_update_progress(session_dir, output)?;
    hooks_common::complete_green_if_all_tests_passing(session_dir, &parsed);
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
