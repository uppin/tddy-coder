//! Reads `projects.yaml` using the same on-disk schema as [`tddy_daemon::project_storage`].
//!
//! **tddy-livekit** cannot depend on **tddy-daemon** (daemon already depends on this crate). Keep this
//! module aligned with `packages/tddy-daemon/src/project_storage.rs` (`ProjectsFile`, `ProjectData`).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// One project row — mirrors `tddy_daemon::project_storage::ProjectData` for YAML compatibility.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectData {
    pub project_id: String,
    pub name: String,
    pub git_url: String,
    pub main_repo_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch_ref: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub host_repo_paths: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ProjectsFile {
    #[serde(default)]
    pub projects: Vec<ProjectData>,
}

const PROJECTS_FILENAME: &str = "projects.yaml";

fn projects_file_path(projects_dir: &Path) -> std::path::PathBuf {
    projects_dir.join(PROJECTS_FILENAME)
}

/// Row count for `projects_dir/projects.yaml`, same semantics as `tddy_daemon::project_storage::read_projects().len()`.
pub(crate) fn owned_project_row_count(projects_dir: &Path) -> anyhow::Result<u64> {
    let path = projects_file_path(projects_dir);
    if !path.exists() {
        log::debug!(
            target: "tddy_livekit::projects_registry",
            "projects.yaml missing at {}; count=0",
            path.display()
        );
        return Ok(0);
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
    let file: ProjectsFile = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("parse {}: {}", path.display(), e))?;
    let n = file.projects.len() as u64;
    log::info!(
        target: "tddy_livekit::projects_registry",
        "read {} project row(s) from {}",
        n,
        path.display()
    );
    Ok(n)
}
