//! How a project row becomes a `ProjectEntry`, and how a listing gathers its peers' rows.
//!
//! Moved from `tddy-session-lifecycle` with the project handlers, their only callers:
//! `merge_listed_projects_with_peers` from `connection_service.rs`, the other two from its
//! `hooks_and_urls` module.

use std::path::Path;

use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_projects::project_storage;
use tddy_service::proto::project::ProjectEntry as ProtoProjectEntry;

/// Merge local `ListProjects` rows with [`EligibleDaemonSource::peer_project_entries`].
pub(super) async fn merge_listed_projects_with_peers(
    eligible: &dyn EligibleDaemonSource,
    session_token: &str,
    local: Vec<ProtoProjectEntry>,
) -> Vec<ProtoProjectEntry> {
    let peer_rows = eligible.peer_project_entries(session_token).await;
    log::debug!(
        target: "tddy_daemon::connection_service",
        "merge_listed_projects_with_peers: local_rows={} peer_rows={} (session_token len={})",
        local.len(),
        peer_rows.len(),
        session_token.len()
    );
    let mut merged = local;
    let n_append = peer_rows.len();
    merged.extend(peer_rows);
    log::info!(
        target: "tddy_daemon::connection_service",
        "merge_listed_projects_with_peers: merged_total={} appended_from_peers={}",
        merged.len(),
        n_append
    );
    merged
}

/// Resolves the default remote name for a registered project, degrading to an empty string when the
/// resolver itself errors (e.g. unreadable `projects.yaml`) so a list RPC never fails on a single
/// bad row. The resolver already falls back to `origin` as the last resort, so the empty case is the
/// rare "registry unreadable" path — clients apply their own `origin` fallback then.
pub(super) fn resolve_default_remote_or_empty(
    projects_dir: &Path,
    project_id: &str,
    repo_root: &Path,
) -> String {
    project_storage::effective_remote_name_for_project(projects_dir, project_id, repo_root)
        .unwrap_or_default()
}

/// Builds a proto [`ProjectEntry`] from a stored [`project_storage::ProjectData`] plus the resolved
/// `default_remote`. Centralizing the mapping keeps every response (ListProjects, CreateProject,
/// AddProjectToHost, SetProjectDefaultBranch) consistent as fields are added.
pub(super) fn project_entry_from(
    p: &project_storage::ProjectData,
    daemon_instance_id: String,
    default_remote: String,
) -> ProtoProjectEntry {
    ProtoProjectEntry {
        project_id: p.project_id.clone(),
        name: p.name.clone(),
        git_url: p.git_url.clone(),
        main_repo_path: p.main_repo_path.clone(),
        daemon_instance_id,
        main_branch_ref: p.main_branch_ref.clone().unwrap_or_default(),
        default_remote,
    }
}
