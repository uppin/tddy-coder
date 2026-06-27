//! `list-actions` / `invoke-action` CLI orchestration (Session Actions PRD).
//!
//! These functions are used as the LOCAL FALLBACK when `TDDY_SOCKET` is not set.
//! The relay path (when the socket is available) is handled in `cli.rs` directly.

use std::path::{Path, PathBuf};

use log::{debug, info};
use serde::Serialize;

use tddy_core::session_actions::{
    classify_session_actions_exit_code, derive_repo_key, invoke_action_core, list_action_summaries,
    repo_actions_root, ActionSummary, DiscoveryQuery, SessionActionsError,
};
use tddy_core::{read_changeset, WorkflowError};

#[derive(Debug, Serialize)]
pub struct ListActionsResponse {
    pub actions: Vec<ActionSummary>,
    pub total: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Local (non-relay) `list-actions` implementation.
pub fn run_list_actions(
    session_dir: &Path,
    path_prefix: Option<&str>,
    query_str: Option<&str>,
    limit: Option<usize>,
    offset: usize,
) -> anyhow::Result<()> {
    info!(
        target: "tddy_tools::session_actions_cli",
        "list-actions (local) session_dir={}",
        session_dir.display()
    );
    let repo_root = load_repo_root(session_dir).map_err(anyhow::Error::from)?;
    let query = DiscoveryQuery {
        path_prefix: path_prefix.map(str::to_owned),
        query: query_str.map(str::to_owned),
        limit,
        offset,
    };
    let tddy_data_dir = resolve_tddy_data_dir();
    let result = list_action_summaries(
        Some(session_dir),
        repo_root.as_deref(),
        &tddy_data_dir,
        &query,
    )
    .map_err(anyhow::Error::from)?;
    let out = ListActionsResponse {
        actions: result.actions,
        total: result.total,
        offset,
        limit,
    };
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

/// Local (non-relay) `invoke-action` implementation.
pub fn run_invoke_action(
    session_dir: &Path,
    action_id: &str,
    data_json: &str,
) -> anyhow::Result<()> {
    debug!(
        target: "tddy_tools::session_actions_cli",
        "invoke-action (local) action_id={} session_dir={}",
        action_id,
        session_dir.display()
    );

    let repo_root = load_repo_root(session_dir).map_err(anyhow::Error::from)?;
    let tddy_data_dir = resolve_tddy_data_dir();
    let store_root = repo_root.as_ref().map(|r| {
        let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
        let key = derive_repo_key(&canon);
        repo_actions_root(&tddy_data_dir, &key)
    });

    match invoke_action_core(
        Some(session_dir),
        store_root.as_deref(),
        repo_root.as_deref(),
        action_id,
        data_json,
    ) {
        Ok(v) => {
            println!("{}", serde_json::to_string(&v)?);
            Ok(())
        }
        Err(e) => {
            let code = classify_session_actions_exit_code(&e);
            eprintln!("{e}");
            std::process::exit(code);
        }
    }
}

/// Resolve the tddy data directory using the profile default or `$HOME/.tddy`.
fn resolve_tddy_data_dir() -> PathBuf {
    tddy_core::output::default_tddy_data_dir().unwrap_or_else(|| {
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
                target: "tddy_tools::session_actions_cli",
                "load_repo_root: repo_path={:?}",
                p.as_ref().map(|x| x.display().to_string())
            );
            Ok(p)
        }
        Err(WorkflowError::ChangesetMissing(_)) => {
            debug!(
                target: "tddy_tools::session_actions_cli",
                "load_repo_root: no changeset.yaml; repo_path unavailable"
            );
            Ok(None)
        }
        Err(e) => Err(SessionActionsError::ChangesetRead(e.to_string())),
    }
}
