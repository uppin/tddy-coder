//! Per-user project registry (`~/.tddy/projects/projects.yaml`).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One project row stored in `projects.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    pub project_id: String,
    pub name: String,
    pub git_url: String,
    pub main_repo_path: String,
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
    let mut projects = read_projects(projects_dir)?;
    projects.push(project);
    write_projects(projects_dir, &projects)
}

/// Find project by id.
pub fn find_project(projects_dir: &Path, project_id: &str) -> anyhow::Result<Option<ProjectData>> {
    let projects = read_projects(projects_dir)?;
    Ok(projects.into_iter().find(|p| p.project_id == project_id))
}
