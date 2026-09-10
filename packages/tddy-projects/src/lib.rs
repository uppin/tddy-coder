//! The repositories a daemon knows, their hosts, and their branches.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 9, serving `project.ProjectService` — family D,
//! 5 methods.
//!
//! # Why a crate of its own rather than joining `tddy-worktree-service`
//!
//! The two are adjacent — a worktree belongs to a project — but **a project outlives every worktree
//! cut from it**, and `ListProjectBranches` reads the project's *main checkout*, not a worktree.
//!
//! There is a second, more practical reason. Node 1's `## Affected Packages` claims the git/worktree
//! subsystem as 8 modules, which includes `project_storage.rs` and `project_provision.rs`; node 8's
//! `## Boundaries` simultaneously declares those two as staying with family D. Two changesets
//! contradict each other. Giving projects their own crate resolves that by subtraction rather than by
//! argument: node 1 takes 6 modules, and these two come here.
//!
//! # `projects.yaml` had a hand-maintained mirror
//!
//! `packages/tddy-livekit/src/projects_registry.rs` reads the same on-disk schema, with a doc comment
//! saying it "mirrors `tddy_daemon::project_storage::ProjectData` for YAML compatibility". That copy
//! exists *because* the original was locked inside a binary crate. With the schema in a library, the
//! mirror can be deleted — recorded rather than done here, because `tddy-livekit` is outside this
//! node's scope.

use std::path::PathBuf;

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

/// Read every project row from `projects_dir/projects.yaml`.
///
/// A store that cannot be read is an error, never an empty list: a daemon that answers "no projects"
/// from an unreadable file tells an operator their projects are gone.
pub fn read_projects(_projects_dir: &std::path::Path) -> Result<Vec<ProjectData>, ProjectError> {
    // TODO(daemon-becomes-wiring): implement
    unimplemented!("read_projects")
}

/// The `project.ProjectService` entry the daemon's wiring layer registers.
pub fn build_project_entry(_projects_dir: PathBuf) -> tddy_rpc::ServiceEntry {
    // TODO(daemon-becomes-wiring): implement
    unimplemented!("build_project_entry")
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
