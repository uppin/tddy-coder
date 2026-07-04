//! Auto-provisioning a project's working copy on the daemon that will run a session.
//!
//! [`StartSession`](crate::connection_service) currently requires the project to be registered
//! locally with an on-disk `main_repo_path`, returning `not_found` / `invalid_argument` otherwise.
//! This module isolates the "make the working copy exist here, cloning it if missing" step so it can
//! be exercised without a live LiveKit room or a full `ConnectionServiceImpl`: cloning and
//! peer-project discovery are injected as closures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tddy_rpc::Status;

use crate::project_storage::{self, ProjectData};

/// Ensure the working copy for `project_id` exists locally, cloning it when missing.
///
/// Resolution:
/// 1. If the project is registered in `projects_dir` and its `main_repo_path` exists on disk, return
///    it unchanged (no clone).
/// 2. If it is registered but the working copy is missing, clone from the stored `git_url` into its
///    registered `main_repo_path`.
/// 3. If it is not registered locally, resolve `name` + `git_url` via `peer_lookup`, clone into
///    `repos_base_dir/<name>`, and register the row (reusing `project_id`).
/// 4. If it is unknown locally and on every peer, return [`tddy_rpc::Code::NotFound`].
///
/// `cloner(git_url, dest)` performs (or fakes) the clone; it must be idempotent for an existing
/// `dest`. `peer_lookup(project_id)` returns `(name, git_url)` for a project known only on a peer.
///
/// `repos_base_dir` is required only for the peer-provisioned clone (case 3); a locally-registered
/// project (cases 1–2) never consults it, so callers that cannot resolve a base path may pass
/// `None` and it only surfaces as an error when a peer clone actually needs it.
pub fn ensure_project_available_locally<C, P>(
    projects_dir: &Path,
    project_id: &str,
    repos_base_dir: Option<&Path>,
    cloner: C,
    peer_lookup: P,
) -> Result<ProjectData, Status>
where
    C: Fn(&str, &Path) -> Result<(), String>,
    P: Fn(&str) -> Option<(String, String)>,
{
    if let Some(project) = project_storage::find_project(projects_dir, project_id)
        .map_err(|e| Status::internal(format!("read project registry: {e}")))?
    {
        let dest = PathBuf::from(&project.main_repo_path);
        if dest.exists() {
            return Ok(project);
        }
        clone_into(&cloner, &project.git_url, &dest)?;
        return Ok(project);
    }

    let (name, git_url) = peer_lookup(project_id).ok_or_else(|| {
        Status::not_found(format!(
            "project not found locally or on any peer: {project_id}"
        ))
    })?;
    let repos_base_dir =
        repos_base_dir.ok_or_else(|| Status::internal("could not resolve repos base path"))?;
    let dest = repos_base_dir.join(&name);
    clone_into(&cloner, &git_url, &dest)?;
    let (project, _created) = project_storage::add_or_get_project(
        projects_dir,
        ProjectData {
            project_id: project_id.to_string(),
            name,
            git_url,
            main_repo_path: dest.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        },
    )
    .map_err(|e| Status::internal(format!("register provisioned project: {e}")))?;
    Ok(project)
}

/// Run `cloner`, mapping a clone failure to a gRPC error rather than masking it.
fn clone_into<C>(cloner: &C, git_url: &str, dest: &Path) -> Result<(), Status>
where
    C: Fn(&str, &Path) -> Result<(), String>,
{
    cloner(git_url, dest)
        .map_err(|e| Status::internal(format!("clone {git_url} into {}: {e}", dest.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Records every clone request and materializes the destination directory so callers can observe
    /// "the working copy now exists" exactly as a real clone would leave it.
    struct RecordingCloner {
        calls: RefCell<Vec<(String, PathBuf)>>,
    }

    impl RecordingCloner {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
            }
        }

        fn clone_fn(&self) -> impl Fn(&str, &Path) -> Result<(), String> + '_ {
            move |git_url: &str, dest: &Path| {
                self.calls
                    .borrow_mut()
                    .push((git_url.to_string(), dest.to_path_buf()));
                std::fs::create_dir_all(dest).map_err(|e| e.to_string())
            }
        }

        fn calls(&self) -> Vec<(String, PathBuf)> {
            self.calls.borrow().clone()
        }
    }

    #[allow(non_snake_case)]
    fn aProject(project_id: &str, name: &str, git_url: &str, main_repo_path: &Path) -> ProjectData {
        ProjectData {
            project_id: project_id.to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path: main_repo_path.to_string_lossy().to_string(),
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        }
    }

    /// A peer lookup that never finds anything.
    fn no_peer(_project_id: &str) -> Option<(String, String)> {
        None
    }

    #[test]
    fn clones_the_project_into_the_base_location_when_registered_but_working_copy_is_missing() {
        // Given a registered project whose working copy directory does not exist on disk
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let repos_base = temp.path().join("repos");
        let dest = repos_base.join("alpha");
        project_storage::add_project(
            &projects_dir,
            aProject("p-alpha", "alpha", "https://example.com/alpha.git", &dest),
        )
        .unwrap();
        assert!(!dest.exists(), "precondition: working copy must be absent");
        let cloner = RecordingCloner::new();

        // When
        let project = ensure_project_available_locally(
            &projects_dir,
            "p-alpha",
            Some(repos_base.as_path()),
            cloner.clone_fn(),
            no_peer,
        )
        .expect("expected the project to be provisioned");

        // Then it clones from the stored git_url into the registered path, which now exists
        assert_eq!(
            cloner.calls(),
            vec![("https://example.com/alpha.git".to_string(), dest.clone())]
        );
        assert_eq!(project.main_repo_path, dest.to_string_lossy());
        assert!(Path::new(&project.main_repo_path).exists());
    }

    #[test]
    fn provisions_an_unregistered_project_by_cloning_from_the_peer_discovered_git_url() {
        // Given an empty local registry and a peer that knows the project
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let repos_base = temp.path().join("repos");
        let cloner = RecordingCloner::new();
        let peer = |project_id: &str| {
            assert_eq!(project_id, "p-beta");
            Some((
                "beta".to_string(),
                "https://example.com/beta.git".to_string(),
            ))
        };

        // When
        let project = ensure_project_available_locally(
            &projects_dir,
            "p-beta",
            Some(repos_base.as_path()),
            cloner.clone_fn(),
            peer,
        )
        .expect("expected the project to be provisioned from the peer");

        // Then it clones into repos_base/<name> and registers the reused project id locally
        let expected_dest = repos_base.join("beta");
        assert_eq!(
            cloner.calls(),
            vec![(
                "https://example.com/beta.git".to_string(),
                expected_dest.clone()
            )]
        );
        assert_eq!(project.project_id, "p-beta");
        assert_eq!(project.main_repo_path, expected_dest.to_string_lossy());
        let registered = project_storage::find_project(&projects_dir, "p-beta")
            .unwrap()
            .expect("project must be registered locally after provisioning");
        assert_eq!(registered.git_url, "https://example.com/beta.git");
    }

    #[test]
    fn returns_not_found_when_the_project_is_unknown_locally_and_on_every_peer() {
        // Given an empty registry and a peer lookup that finds nothing
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let repos_base = temp.path().join("repos");
        let cloner = RecordingCloner::new();

        // When
        let err = ensure_project_available_locally(
            &projects_dir,
            "p-unknown",
            Some(repos_base.as_path()),
            cloner.clone_fn(),
            no_peer,
        )
        .expect_err("expected NotFound for an unknown project");

        // Then
        assert_eq!(err.code, tddy_rpc::Code::NotFound);
        assert!(
            cloner.calls().is_empty(),
            "must not clone an unknown project"
        );
    }

    #[test]
    fn errors_when_a_peer_provisioned_clone_has_no_base_path() {
        // Given an empty local registry and a peer that knows the project, but no resolvable base
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let cloner = RecordingCloner::new();
        let peer = |_: &str| {
            Some((
                "delta".to_string(),
                "https://example.com/delta.git".to_string(),
            ))
        };

        // When the project must be cloned from a peer but no base path is available
        let err = ensure_project_available_locally(
            &projects_dir,
            "p-delta",
            None,
            cloner.clone_fn(),
            peer,
        )
        .expect_err("expected an error when the clone base path cannot be resolved");

        // Then it reports the missing base path and does not clone
        assert_eq!(err.code, tddy_rpc::Code::Internal);
        assert!(
            err.message.contains("repos base path"),
            "message was '{}'",
            err.message
        );
        assert!(
            cloner.calls().is_empty(),
            "must not clone without a base path"
        );
    }

    #[test]
    fn does_not_re_clone_when_the_working_copy_already_exists() {
        // Given a registered project whose working copy is already on disk
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let repos_base = temp.path().join("repos");
        let dest = repos_base.join("gamma");
        std::fs::create_dir_all(&dest).unwrap();
        project_storage::add_project(
            &projects_dir,
            aProject("p-gamma", "gamma", "https://example.com/gamma.git", &dest),
        )
        .unwrap();
        let cloner = RecordingCloner::new();

        // When
        let project = ensure_project_available_locally(
            &projects_dir,
            "p-gamma",
            Some(repos_base.as_path()),
            cloner.clone_fn(),
            no_peer,
        )
        .expect("expected the existing working copy to be returned");

        // Then the existing checkout is returned untouched
        assert!(
            cloner.calls().is_empty(),
            "must not re-clone an existing working copy"
        );
        assert_eq!(project.main_repo_path, dest.to_string_lossy());
    }
}
