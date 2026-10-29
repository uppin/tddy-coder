use std::collections::HashMap;
use std::path::Path;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};

/// One session as the target resolver reads it: its `.session.yaml`, its changeset's branch, and the
/// ref its worktree is diffed against.
pub(super) struct SessionRow {
    pub(super) entry: SessionEntry,
    pub(super) branch: String,
    pub(super) base_ref: String,
}
pub(super) fn session_rows(
    sessions_base: &Path,
    tddy_data_dir: &Path,
    os_user: &str,
) -> anyhow::Result<Vec<SessionRow>> {
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir));
    // A project's main branch is the diff base of every session in it, so each is read once.
    let mut main_branch_refs: HashMap<String, Option<String>> = HashMap::new();
    let mut rows = Vec::new();
    for entry in list_sessions_in_dir(sessions_base)? {
        let session_dir = unified_session_dir_path(sessions_base, &entry.session_id);
        let changeset = tddy_core::read_changeset(&session_dir).ok();
        let branch = changeset
            .as_ref()
            .and_then(|c| c.branch.clone())
            .unwrap_or_default();
        let recorded_base = changeset
            .and_then(|c| c.effective_worktree_integration_base_ref)
            .filter(|r| !r.trim().is_empty());
        let base_ref = match recorded_base {
            Some(base) => base,
            None => main_branch_refs
                .entry(entry.project_id.clone())
                .or_insert_with(|| {
                    let dir = projects_dir.as_ref()?;
                    tddy_projects::project_storage::find_project(dir, &entry.project_id)
                        .ok()
                        .flatten()
                        .and_then(|p| p.main_branch_ref)
                })
                .clone()
                .unwrap_or_default(),
        };
        rows.push(SessionRow {
            entry,
            branch,
            base_ref,
        });
    }
    Ok(rows)
}
