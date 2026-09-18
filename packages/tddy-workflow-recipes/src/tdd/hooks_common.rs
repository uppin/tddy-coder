//! Shared helpers for [`super::hooks::TddWorkflowHooks`] and [`crate::tdd_small::hooks::TddSmallWorkflowHooks`]
//! to avoid behavioral drift between classic TDD and `tdd-small`.
//!
//! The lifecycle halves live in submodules and are re-exported here, so every existing
//! `hooks_common::…` call site resolves unchanged. What stays in this file is what all three
//! submodules read: the session-document readers, the model and session-id resolvers, and the
//! changeset writer.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tddy_core::changeset::{
    get_session_for_tag, read_changeset, update_state, write_changeset, Changeset,
};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;

use crate::SessionArtifactManifest;

mod before;
pub(crate) use before::*;

mod after;
pub(crate) use after::*;

mod sinks;
pub(crate) use sinks::*;

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

/// The log labels `on_error` emits. They are named at the call site rather than passed positionally
/// because two of the three are same-typed prefixes a transposition would swap silently, and
/// `log_target` preserves the `module_path!()` each workflow's statements carried before they were
/// shared — the same reason `before_green` takes one.
pub(crate) struct OnErrorLabels {
    pub(crate) log_target: &'static str,
    pub(crate) task_failed_prefix: &'static str,
    pub(crate) persist_failed_prefix: &'static str,
}

/// Persist the `Failed` state and announce it. `labels` keeps each workflow's two log lines
/// character for character what they were before this helper was shared.
pub(crate) fn on_error(
    context: &Context,
    event_tx: Option<&mpsc::Sender<WorkflowEvent>>,
    error: &(dyn Error + Send + Sync),
    labels: OnErrorLabels,
) {
    log::error!(
        target: labels.log_target,
        "{} workflow task failed: {}",
        labels.task_failed_prefix,
        error
    );
    let session_dir: Option<PathBuf> = context
        .get_sync("session_dir")
        .or_else(|| context.get_sync("output_dir"));
    let Some(ref dir) = session_dir else {
        return;
    };
    let Ok(mut cs) = read_changeset(dir) else {
        return;
    };
    let from = cs.state.current.to_string();
    update_state(&mut cs, WorkflowState::new("Failed"));
    if let Err(e) = write_changeset(dir, &cs) {
        log::warn!(
            target: labels.log_target,
            "{} on_error: could not persist Failed state: {} (session_dir={})",
            labels.persist_failed_prefix,
            e,
            dir.display()
        );
        return;
    }
    if let Some(tx) = event_tx {
        let _ = tx.send(WorkflowEvent::StateChange {
            from,
            to: "Failed".to_string(),
        });
    }
}
