//! Per-user project registry (`~/.tddy/projects/projects.yaml`).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One project row stored in `projects.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    pub project_id: String,
    pub name: String,
    pub git_url: String,
    pub main_repo_path: String,
    /// Remote-tracking ref used as the integration base for worktrees (`origin/main`, etc.).
    /// Absent entries behave as [`tddy_core::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch_ref: Option<String>,
    /// Per-host (or per-daemon-instance) checkout paths for the same logical `project_id`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub host_repo_paths: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsFile {
    #[serde(default)]
    pub projects: Vec<ProjectData>,
}

const PROJECTS_FILENAME: &str = "projects.yaml";

fn projects_file_path(projects_dir: &Path) -> std::path::PathBuf {
    projects_dir.join(PROJECTS_FILENAME)
}

/// Read all projects. Returns empty vec if file is missing.
pub fn read_projects(projects_dir: &Path) -> anyhow::Result<Vec<ProjectData>> {
    let path = projects_file_path(projects_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
    let file: ProjectsFile = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("parse {}: {}", path.display(), e))?;
    Ok(file.projects)
}

/// Write the full project list (replace).
pub fn write_projects(projects_dir: &Path, projects: &[ProjectData]) -> anyhow::Result<()> {
    std::fs::create_dir_all(projects_dir)
        .map_err(|e| anyhow::anyhow!("create {}: {}", projects_dir.display(), e))?;
    let path = projects_file_path(projects_dir);
    let file = ProjectsFile {
        projects: projects.to_vec(),
    };
    let contents =
        serde_yaml::to_string(&file).map_err(|e| anyhow::anyhow!("serialize projects: {}", e))?;
    std::fs::write(&path, contents).map_err(|e| anyhow::anyhow!("write {}: {}", path.display(), e))
}

/// Append one project after reading existing.
pub fn add_project(projects_dir: &Path, project: ProjectData) -> anyhow::Result<()> {
    log::info!("add_project: project_id={}", project.project_id);
    if let Some(ref r) = project.main_branch_ref {
        tddy_core::validate_integration_base_ref(r)
            .map_err(|e| anyhow::anyhow!("invalid main_branch_ref: {}", e))?;
    }
    let mut projects = read_projects(projects_dir)?;
    projects.push(project);
    write_projects(projects_dir, &projects)
}

/// Find project by id.
pub fn find_project(projects_dir: &Path, project_id: &str) -> anyhow::Result<Option<ProjectData>> {
    let projects = read_projects(projects_dir)?;
    Ok(projects.into_iter().find(|p| p.project_id == project_id))
}

/// Resolves the git integration base ref for worktree setup for a registered project.
///
/// Legacy rows without [`ProjectData::main_branch_ref`] must resolve to
/// [`tddy_core::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`].
pub fn effective_integration_base_ref_for_project(
    projects_dir: &Path,
    project_id: &str,
) -> anyhow::Result<String> {
    log::debug!(
        "effective_integration_base_ref_for_project: project_id={}",
        project_id
    );
    let project = find_project(projects_dir, project_id)?
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    match &project.main_branch_ref {
        Some(r) => {
            tddy_core::validate_integration_base_ref(r)
                .map_err(|e| anyhow::anyhow!("invalid main_branch_ref: {}", e))?;
            log::info!(
                "effective_integration_base_ref_for_project: project_id={} ref={}",
                project_id,
                r
            );
            Ok(r.clone())
        }
        None => {
            log::info!(
                "effective_integration_base_ref_for_project: project_id={} using documented default",
                project_id
            );
            Ok(tddy_core::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF.to_string())
        }
    }
}

/// Resolved `main_repo_path` for `project_id` on `host_key` (simulated host or daemon instance id).
///
/// Multi-host: returns [`ProjectData::host_repo_paths`]\[host_key] when non-empty, else
/// [`ProjectData::main_repo_path`].
pub fn main_repo_path_for_host(
    projects_dir: &Path,
    project_id: &str,
    host_key: &str,
) -> anyhow::Result<Option<String>> {
    let p = find_project(projects_dir, project_id)?;
    Ok(p.map(|p| {
        if let Some(path) = p.host_repo_paths.get(host_key) {
            if !path.trim().is_empty() {
                log::debug!(
                    "main_repo_path_for_host: host_repo_paths[{host_key}] project_id={}",
                    p.project_id
                );
                return path.clone();
            }
        }
        log::debug!(
            "main_repo_path_for_host: legacy main_repo_path project_id={} host_key={}",
            p.project_id,
            host_key
        );
        p.main_repo_path.clone()
    }))
}

#[cfg(test)]
mod per_host_path_unit_tests {
    use super::*;
    use std::collections::HashMap;

    /// Per-host map wins over legacy `main_repo_path` for distinct hosts.
    #[test]
    fn main_repo_path_for_host_returns_host_map_entry_not_only_legacy() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let mut host_repo_paths = HashMap::new();
        host_repo_paths.insert("unit-host-x".to_string(), "/x/checkout".to_string());
        host_repo_paths.insert("unit-host-y".to_string(), "/y/checkout".to_string());
        let project = ProjectData {
            project_id: "p1".to_string(),
            name: "n".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: "/legacy".to_string(),
            main_branch_ref: None,
            host_repo_paths,
        };
        write_projects(&projects_dir, &[project]).unwrap();
        let px = main_repo_path_for_host(&projects_dir, "p1", "unit-host-x")
            .unwrap()
            .unwrap();
        let py = main_repo_path_for_host(&projects_dir, "p1", "unit-host-y")
            .unwrap()
            .unwrap();
        assert_ne!(
            px, py,
            "same project_id must resolve to different paths per host_key"
        );
        assert_eq!(px, "/x/checkout");
        assert_eq!(py, "/y/checkout");
    }
}

#[cfg(test)]
mod project_integration_base_acceptance_tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    /// Legacy `projects.yaml` without `main_branch_ref` must resolve to the documented default ref.
    #[test]
    fn legacy_project_without_base_ref_uses_documented_default() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let yaml = r#"projects:
- project_id: "p-legacy"
  name: "n"
  git_url: "https://example.com/r.git"
  main_repo_path: "/tmp/r"
"#;
        fs::write(projects_file_path(&projects_dir), yaml).unwrap();

        let eff = effective_integration_base_ref_for_project(&projects_dir, "p-legacy").unwrap();
        assert_eq!(eff, tddy_core::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF);
    }

    /// Invalid `main_branch_ref` values must be rejected before YAML mutation.
    #[test]
    fn invalid_base_ref_rejected_at_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let project = ProjectData {
            project_id: "bad-ref".to_string(),
            name: "n".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: "/tmp/r".to_string(),
            main_branch_ref: Some("refs/heads/main".to_string()),
            host_repo_paths: HashMap::new(),
        };
        let r = add_project(&projects_dir, project);
        assert!(
            r.is_err(),
            "invalid integration base ref must be rejected before persistence: {:?}",
            r
        );
        assert!(
            read_projects(&projects_dir).unwrap().is_empty(),
            "projects.yaml must not be written when validation fails"
        );
    }
}
