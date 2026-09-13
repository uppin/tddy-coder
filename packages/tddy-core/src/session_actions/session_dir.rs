//! Listing and invoking a session's actions from a session directory alone.
//!
//! This is the path taken when there is no host to relay to: the repo root comes from the
//! session's `changeset.yaml`, the action store from the tddy data directory, and the result is
//! the same JSON shape a relayed `list-actions` answers with. `tddy_tools::session_actions_cli`
//! wraps these in the CLI's argument parsing, stdout and exit codes.

use std::path::{Path, PathBuf};

use log::debug;
use serde::Serialize;

use super::{
    derive_repo_key, invoke_action_core, list_action_summaries, repo_actions_root, ActionSummary,
    DiscoveryQuery, SessionActionsError,
};
use crate::{read_changeset, WorkflowError};

/// The `list-actions` JSON response: a page of summaries plus the paging coordinates it answers.
#[derive(Debug, Serialize)]
pub struct ListActionsResponse {
    pub actions: Vec<ActionSummary>,
    pub total: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// List the actions discoverable from `session_dir`, paged by `query`.
pub fn list_actions_in_session_dir(
    session_dir: &Path,
    query: &DiscoveryQuery,
) -> Result<ListActionsResponse, SessionActionsError> {
    let repo_root = load_repo_root(session_dir)?;
    let tddy_data_dir = resolve_tddy_data_dir();
    let result = list_action_summaries(
        Some(session_dir),
        repo_root.as_deref(),
        &tddy_data_dir,
        query,
    )?;
    Ok(ListActionsResponse {
        actions: result.actions,
        total: result.total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// Invoke `action_id` with `data_json` against the actions discoverable from `session_dir`.
pub fn invoke_action_in_session_dir(
    session_dir: &Path,
    action_id: &str,
    data_json: &str,
) -> Result<serde_json::Value, SessionActionsError> {
    let repo_root = load_repo_root(session_dir)?;
    let tddy_data_dir = resolve_tddy_data_dir();
    let store_root = repo_root.as_ref().map(|r| {
        let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
        let key = derive_repo_key(&canon);
        repo_actions_root(&tddy_data_dir, &key)
    });

    invoke_action_core(
        Some(session_dir),
        store_root.as_deref(),
        repo_root.as_deref(),
        action_id,
        data_json,
    )
}

/// Resolve the tddy data directory using the profile default or `$HOME/.tddy`.
fn resolve_tddy_data_dir() -> PathBuf {
    crate::output::default_tddy_data_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".tddy")
    })
}

fn load_repo_root(session_dir: &Path) -> Result<Option<PathBuf>, SessionActionsError> {
    match read_changeset(session_dir) {
        Ok(cs) => {
            let p = cs
                .repo_path
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from);
            debug!(
                target: "tddy_core::session_actions::session_dir",
                "load_repo_root: repo_path={:?}",
                p.as_ref().map(|x| x.display().to_string())
            );
            Ok(p)
        }
        Err(WorkflowError::ChangesetMissing(_)) => {
            debug!(
                target: "tddy_core::session_actions::session_dir",
                "load_repo_root: no changeset.yaml; repo_path unavailable"
            );
            Ok(None)
        }
        Err(e) => Err(SessionActionsError::ChangesetRead(e.to_string())),
    }
}
