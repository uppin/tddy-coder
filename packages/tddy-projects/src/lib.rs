//! The repositories a daemon knows, their hosts, and their branches.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 9, serving `project.ProjectService` — family D,
//! 5 methods.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

/// One project row, as `projects.yaml` stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectData {
    pub id: String,
    pub name: String,
    pub main_repo_path: PathBuf,
    pub default_branch: String,
}

/// Why a project operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("no project {project_id}")]
    NoSuchProject { project_id: String },
    #[error("{path} is not a git repository")]
    NotARepository { path: String },
    #[error("projects.yaml at {path} could not be read: {reason}")]
    StoreUnreadable { path: String, reason: String },
}

#[derive(Debug, Deserialize)]
struct ProjectsFileRow {
    project_id: String,
    name: String,
    main_repo_path: String,
    #[serde(default)]
    main_branch_ref: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ProjectsFile {
    #[serde(default)]
    projects: Vec<ProjectsFileRow>,
}

const PROJECTS_FILENAME: &str = "projects.yaml";

fn projects_file_path(projects_dir: &std::path::Path) -> std::path::PathBuf {
    projects_dir.join(PROJECTS_FILENAME)
}

/// Read every project row from `projects_dir/projects.yaml`.
///
/// A store that cannot be read is an error, never an empty list: a daemon that answers "no projects"
/// from an unreadable file tells an operator their projects are gone.
pub fn read_projects(projects_dir: &std::path::Path) -> Result<Vec<ProjectData>, ProjectError> {
    let path = projects_file_path(projects_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path).map_err(|e| ProjectError::StoreUnreadable {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    let file: ProjectsFile = serde_yaml::from_str(&contents).map_err(|e| {
        ProjectError::StoreUnreadable {
            path: path.display().to_string(),
            reason: e.to_string(),
        }
    })?;
    Ok(file
        .projects
        .into_iter()
        .map(|row| ProjectData {
            id: row.project_id,
            name: row.name,
            main_repo_path: PathBuf::from(row.main_repo_path),
            default_branch: row
                .main_branch_ref
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| "origin/main".to_string()),
        })
        .collect())
}

struct ProjectServiceStub;

#[async_trait::async_trait]
impl tddy_service::proto::project::ProjectService for ProjectServiceStub {
    async fn list_projects(
        &self,
        _request: tddy_rpc::Request<tddy_service::proto::project::ListProjectsRequest>,
    ) -> Result<
        tddy_rpc::Response<tddy_service::proto::project::ListProjectsResponse>,
        tddy_rpc::Status,
    > {
        Err(tddy_rpc::Status::unimplemented("project.ProjectService migration in progress"))
    }

    async fn create_project(
        &self,
        _request: tddy_rpc::Request<tddy_service::proto::project::CreateProjectRequest>,
    ) -> Result<
        tddy_rpc::Response<tddy_service::proto::project::CreateProjectResponse>,
        tddy_rpc::Status,
    > {
        Err(tddy_rpc::Status::unimplemented("project.ProjectService migration in progress"))
    }

    async fn add_project_to_host(
        &self,
        _request: tddy_rpc::Request<tddy_service::proto::project::AddProjectToHostRequest>,
    ) -> Result<
        tddy_rpc::Response<tddy_service::proto::project::AddProjectToHostResponse>,
        tddy_rpc::Status,
    > {
        Err(tddy_rpc::Status::unimplemented("project.ProjectService migration in progress"))
    }

    async fn list_project_branches(
        &self,
        _request: tddy_rpc::Request<tddy_service::proto::project::ListProjectBranchesRequest>,
    ) -> Result<
        tddy_rpc::Response<tddy_service::proto::project::ListProjectBranchesResponse>,
        tddy_rpc::Status,
    > {
        Err(tddy_rpc::Status::unimplemented("project.ProjectService migration in progress"))
    }

    async fn set_project_default_branch(
        &self,
        _request: tddy_rpc::Request<tddy_service::proto::project::SetProjectDefaultBranchRequest>,
    ) -> Result<
        tddy_rpc::Response<tddy_service::proto::project::SetProjectDefaultBranchResponse>,
        tddy_rpc::Status,
    > {
        Err(tddy_rpc::Status::unimplemented("project.ProjectService migration in progress"))
    }
}

/// The `project.ProjectService` entry the daemon's wiring layer registers.
pub fn build_project_entry(_projects_dir: PathBuf) -> tddy_rpc::ServiceEntry {
    tddy_rpc::ServiceEntry {
        name: "project.ProjectService",
        service: Arc::new(tddy_service::ProjectServiceServer::new(ProjectServiceStub))
            as Arc<dyn tddy_rpc::RpcService>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_family_d_moves_to() {
        // When
        let entry = build_project_entry(PathBuf::from("/tmp/projects"));

        // Then
        assert_eq!(entry.name, "project.ProjectService");
    }

    /// An unreadable store is an error, never an empty answer. A daemon that reports "no projects"
    /// from a corrupt file tells an operator their projects are gone.
    #[test]
    fn refuses_to_read_a_store_it_cannot_parse() {
        // Given a projects.yaml that is not YAML
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("projects.yaml"), "\x00\x01not yaml").unwrap();

        // When
        let outcome = read_projects(dir.path());

        // Then
        assert!(matches!(outcome, Err(ProjectError::StoreUnreadable { .. })));
    }

    /// A directory with no `projects.yaml` has no projects yet — distinct from one it cannot read.
    #[test]
    fn reads_no_projects_from_a_directory_that_has_none() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let projects = read_projects(dir.path()).unwrap();

        // Then
        assert!(projects.is_empty());
    }
}
