//! How a `ListProjects` answer treats soft-deleted projects, so a tombstone wins over a host that
//! missed it.
//!
//! The merged rows — this host's and every reachable peer's, tombstones included — are grouped by
//! `project_id`, and the newest delete or restore event among a project's rows decides whether the
//! logical project is deleted ([`ProjectLifecycle::newest`]). Every row of the project is listed
//! with that lifecycle, this host adopts it into its own row when it is newer (stopping the
//! project's main here if the adopted event is a delete), and a deleted project is left out unless
//! the caller asked for deleted projects.

use std::collections::HashMap;
use std::path::Path;

use tddy_projects::project_storage::{self, ProjectLifecycle};
use tddy_service::proto::project::ProjectEntry;
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;

use super::project_sessions::stop_project_home_sessions;

fn lifecycle_of(entry: &ProjectEntry) -> ProjectLifecycle {
    ProjectLifecycle::from_wire(&entry.deleted_at, &entry.deleted_by, &entry.restored_at)
}

fn stamp(entry: &mut ProjectEntry, lifecycle: &ProjectLifecycle) {
    entry.deleted_at = lifecycle.deleted_at.clone().unwrap_or_default();
    entry.deleted_by = lifecycle.deleted_by.clone().unwrap_or_default();
    entry.restored_at = lifecycle.restored_at.clone().unwrap_or_default();
}

/// This host's own rows only: no reconciliation, deleted rows kept only when asked for.
pub(super) fn local_listing(rows: Vec<ProjectEntry>, include_deleted: bool) -> Vec<ProjectEntry> {
    rows.into_iter()
        .filter(|row| include_deleted || row.deleted_at.is_empty())
        .collect()
}

/// Reconcile a merged listing as the module docs say. `local_id` names this host's rows, whose
/// registry is `projects_dir` and whose sessions live under `sessions_base`. A failure to heal this
/// host's row is logged, never fatal: the listing still answers with the reconciled state.
pub(super) async fn reconciled_listing(
    terminals: &DaemonTerminalSessionStore,
    rows: Vec<ProjectEntry>,
    include_deleted: bool,
    local_id: &str,
    projects_dir: &Path,
    sessions_base: &Path,
) -> Vec<ProjectEntry> {
    let mut by_project: HashMap<&str, Vec<ProjectLifecycle>> = HashMap::new();
    for row in &rows {
        by_project
            .entry(row.project_id.as_str())
            .or_default()
            .push(lifecycle_of(row));
    }
    let winners: HashMap<String, ProjectLifecycle> = by_project
        .into_iter()
        .map(|(id, lifecycles)| (id.to_string(), ProjectLifecycle::newest(lifecycles.iter())))
        .collect();

    let mut listed = Vec::with_capacity(rows.len());
    for mut row in rows {
        let winner = &winners[&row.project_id];
        if row.daemon_instance_id == local_id && lifecycle_of(&row) != *winner {
            heal_local_row(
                terminals,
                &row.project_id,
                winner,
                projects_dir,
                sessions_base,
            )
            .await;
        }
        stamp(&mut row, winner);
        if include_deleted || !winner.is_deleted() {
            listed.push(row);
        }
    }
    listed
}

async fn heal_local_row(
    terminals: &DaemonTerminalSessionStore,
    project_id: &str,
    winner: &ProjectLifecycle,
    projects_dir: &Path,
    sessions_base: &Path,
) {
    match project_storage::adopt_project_lifecycle(projects_dir, project_id, winner) {
        Ok(true) if winner.is_deleted() => {
            if let Err(status) =
                stop_project_home_sessions(terminals, sessions_base, project_id).await
            {
                log::warn!(
                    "ListProjects: adopted a delete of {project_id} but could not stop its main: {}",
                    status.message
                );
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("ListProjects: could not adopt {project_id}'s lifecycle: {e}"),
    }
}
