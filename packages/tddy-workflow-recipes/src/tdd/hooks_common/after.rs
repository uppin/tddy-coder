//! The `after_*` helpers both hook workflows share: each parses one goal's structured output and
//! writes its artifacts and state transition. The parent `hooks_common` re-exports them, so every
//! `hooks_common::after_*` call site resolves unchanged.

use std::error::Error;
use std::path::Path;

use tddy_core::changeset::{
    append_session_and_update_state, read_changeset, update_state, BranchWorktreeIntent, Changeset,
};
use tddy_core::error::WorkflowError;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;

use super::write_changeset_logged;
use crate::parser::{
    parse_green_response, parse_planning_response_with_base, parse_red_response,
    parse_refactor_response, parse_update_docs_response, GreenOutput, PlanningOutput,
};
use crate::writer::{
    update_acceptance_tests_file, update_progress_file, write_artifacts, write_progress_file,
    write_red_output_file,
};
use crate::SessionArtifactManifest;

/// `log_prefix` and `session_tag` carry the only differences between the two workflows: the prefix
/// each one logs under, and the tag it records the planning session with — classic TDD's `"plan"`
/// against `tdd-small`'s start goal. The tag is deliberately not derived from the recipe here:
/// classic TDD's `start_goal()` is not `"plan"`, so deriving it would silently rewrite its
/// changeset entries.
pub(crate) fn after_plan(
    manifest: &dyn SessionArtifactManifest,
    session_dir: &Path,
    context: &Context,
    log_target: &'static str,
    log_prefix: &'static str,
    session_tag: &str,
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
        target: log_target,
        "{} after_plan writing session document basename={:?} under {:?}",
        log_prefix,
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
    if session_exists {
        update_state(&mut cs, WorkflowState::new("Planned"));
    } else {
        append_session_and_update_state(
            &mut cs,
            session_id,
            session_tag,
            WorkflowState::new("Planned"),
            &backend_name,
            Some("system-prompt-plan.md".to_string()),
        );
    }
    write_changeset_logged(session_dir, &cs, "after_plan Planned");
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
