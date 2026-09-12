//! The repositories a daemon knows, their hosts, and their branches.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 9, serving `project.ProjectService` — family D,
//! 5 methods.

pub mod project_provision;
pub mod project_storage;

mod handler;
mod service;

pub use handler::ProjectHandler;
pub use service::{build_project_entry, ProjectServiceImpl};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn names_the_service_family_d_moves_to() {
        struct Noop;
        #[async_trait::async_trait]
        impl ProjectHandler for Noop {
            async fn list_projects(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::project::ListProjectsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::project::ListProjectsResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn create_project(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::project::CreateProjectRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::project::CreateProjectResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn add_project_to_host(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::project::AddProjectToHostRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::project::AddProjectToHostResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn list_project_branches(
                &self,
                _request: tddy_rpc::Request<
                    tddy_service::proto::project::ListProjectBranchesRequest,
                >,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::project::ListProjectBranchesResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn set_project_default_branch(
                &self,
                _request: tddy_rpc::Request<
                    tddy_service::proto::project::SetProjectDefaultBranchRequest,
                >,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::project::SetProjectDefaultBranchResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
        }

        let entry = build_project_entry(ProjectServiceImpl::new(Arc::new(Noop)));
        assert_eq!(entry.name, "project.ProjectService");
    }

    /// An unreadable store is an error, never an empty answer.
    #[test]
    fn refuses_to_read_a_store_it_cannot_parse() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("projects.yaml"), "\x00\x01not yaml").unwrap();
        let outcome = project_storage::read_projects(dir.path());
        assert!(outcome.is_err());
    }

    #[test]
    fn reads_no_projects_from_a_directory_that_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let projects = project_storage::read_projects(dir.path()).unwrap();
        assert!(projects.is_empty());
    }
}
