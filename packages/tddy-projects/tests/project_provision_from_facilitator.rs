//! `ensure_project_available_from_facilitator`: how an owning daemon makes a project's working copy
//! exist here — returning a registered copy, re-cloning a registered one that is missing, or
//! cloning an unregistered project from the facilitating daemon and registering it (PRD AC37).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tddy_projects::project_provision::ensure_project_available_from_facilitator;
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Code, Status};

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";
const FORGE_URL: &str = "https://example.com/alpha.git";
const FACILITATOR_URL: &str = "daemon-facilitator:alpha";

/// Records every clone request; a succeeding clone materializes the destination, as a real one
/// would, and a failing one leaves nothing behind.
struct RecordingCloner {
    calls: RefCell<Vec<(String, PathBuf)>>,
    failure: Option<String>,
}

impl RecordingCloner {
    fn succeeding() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failure: None,
        }
    }

    fn failing_with(message: &str) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failure: Some(message.to_string()),
        }
    }

    fn clone_fn(&self) -> impl Fn(&str, &Path) -> Result<(), String> + '_ {
        move |git_url: &str, dest: &Path| {
            self.calls
                .borrow_mut()
                .push((git_url.to_string(), dest.to_path_buf()));
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => std::fs::create_dir_all(dest).map_err(|e| e.to_string()),
            }
        }
    }

    fn calls(&self) -> Vec<(String, PathBuf)> {
        self.calls.borrow().clone()
    }
}

/// A host's directories: the project registry and the repos base clones land under.
struct Host {
    _root: tempfile::TempDir,
    projects_dir: PathBuf,
    repos_base: PathBuf,
}

fn a_host() -> Host {
    let root = tempfile::tempdir().unwrap();
    let projects_dir = root.path().join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    let repos_base = root.path().join("repos");
    Host {
        projects_dir,
        repos_base,
        _root: root,
    }
}

impl Host {
    /// Registers the project with its working copy at `repos/alpha`, which is not created.
    fn with_registered_project(self) -> Self {
        project_storage::add_project(
            &self.projects_dir,
            ProjectData {
                project_id: PROJECT_ID.to_string(),
                name: "alpha".to_string(),
                git_url: FORGE_URL.to_string(),
                main_repo_path: self.registered_working_copy().to_string_lossy().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: HashMap::new(),
                accounts: Vec::new(),
                local_gh: false,
                home_session_id: None,
            },
        )
        .unwrap();
        self
    }

    fn with_working_copy_on_disk(self) -> Self {
        std::fs::create_dir_all(self.registered_working_copy()).unwrap();
        self
    }

    fn with_unreadable_registry(self) -> Self {
        std::fs::write(
            self.projects_dir.join("projects.yaml"),
            "projects: [not: a, row\n",
        )
        .unwrap();
        self
    }

    /// Points the registry under a regular file, so it reads as empty but can never be written.
    fn with_unwritable_registry(mut self) -> Self {
        let blocker = self.projects_dir.join("not-a-directory");
        std::fs::write(&blocker, "x").unwrap();
        self.projects_dir = blocker.join("projects");
        self
    }

    fn registered_working_copy(&self) -> PathBuf {
        self.repos_base.join("alpha")
    }

    fn provision(
        &self,
        repos_base: Option<&Path>,
        cloner: &RecordingCloner,
    ) -> Result<ProjectData, Status> {
        ensure_project_available_from_facilitator(
            &self.projects_dir,
            PROJECT_ID,
            repos_base,
            FACILITATOR_URL,
            cloner.clone_fn(),
        )
    }

    fn provision_under_repos_base(&self, cloner: &RecordingCloner) -> Result<ProjectData, Status> {
        self.provision(Some(self.repos_base.as_path()), cloner)
    }

    fn stored_row(&self) -> Option<ProjectData> {
        project_storage::find_project(&self.projects_dir, PROJECT_ID).expect("read registry")
    }
}

#[test]
fn a_registered_project_whose_working_copy_exists_is_returned_without_cloning() {
    // Given
    let host = a_host()
        .with_registered_project()
        .with_working_copy_on_disk();
    let cloner = RecordingCloner::succeeding();

    // When
    let project = host
        .provision_under_repos_base(&cloner)
        .expect("the registered project is available");

    // Then
    assert_eq!(cloner.calls(), vec![]);
    assert_eq!(
        project.main_repo_path,
        host.registered_working_copy().to_string_lossy()
    );
}

#[test]
fn a_registered_project_whose_working_copy_is_missing_is_re_cloned_from_its_stored_git_url() {
    // Given
    let host = a_host().with_registered_project();
    let cloner = RecordingCloner::succeeding();

    // When
    host.provision_under_repos_base(&cloner)
        .expect("the registered project is re-cloned");

    // Then
    assert_eq!(
        cloner.calls(),
        vec![(FORGE_URL.to_string(), host.registered_working_copy())]
    );
}

#[test]
fn a_registered_project_never_consults_the_repos_base() {
    // Given
    let host = a_host().with_registered_project();
    let cloner = RecordingCloner::succeeding();

    // When
    let project = host
        .provision(None, &cloner)
        .expect("a registered project needs no repos base");

    // Then
    assert_eq!(project.git_url, FORGE_URL);
}

#[test]
fn a_failed_re_clone_of_a_registered_project_is_reported_as_internal() {
    // Given
    let host = a_host().with_registered_project();
    let cloner = RecordingCloner::failing_with("connection refused");

    // When
    let err = host
        .provision_under_repos_base(&cloner)
        .expect_err("a failed clone must not be masked");

    // Then
    assert_eq!(err.code, Code::Internal);
    assert!(
        err.message.contains("connection refused"),
        "message was '{}'",
        err.message
    );
}

#[test]
fn an_unregistered_project_is_cloned_from_the_facilitator_into_the_repos_base_under_its_id() {
    // Given
    let host = a_host();
    let cloner = RecordingCloner::succeeding();

    // When
    host.provision_under_repos_base(&cloner)
        .expect("the project is provisioned from the facilitator");

    // Then
    assert_eq!(
        cloner.calls(),
        vec![(
            FACILITATOR_URL.to_string(),
            host.repos_base.join(PROJECT_ID)
        )]
    );
}

#[test]
fn an_unregistered_project_is_registered_with_the_facilitator_as_its_origin() {
    // Given
    let host = a_host();
    let cloner = RecordingCloner::succeeding();

    // When
    host.provision_under_repos_base(&cloner)
        .expect("the project is provisioned from the facilitator");

    // Then
    let row = host.stored_row().expect("the project is registered here");
    assert_eq!(
        (row.name.as_str(), row.git_url.as_str(), row.main_repo_path),
        (
            PROJECT_ID,
            FACILITATOR_URL,
            host.repos_base
                .join(PROJECT_ID)
                .to_string_lossy()
                .to_string()
        )
    );
}

#[test]
fn an_unregistered_project_without_a_repos_base_is_refused_without_cloning() {
    // Given
    let host = a_host();
    let cloner = RecordingCloner::succeeding();

    // When
    let err = host
        .provision(None, &cloner)
        .expect_err("a facilitator clone needs a repos base");

    // Then
    assert_eq!(
        (
            err.code,
            err.message.contains("repos base path"),
            cloner.calls()
        ),
        (Code::Internal, true, vec![])
    );
}

#[test]
fn a_failed_clone_from_the_facilitator_registers_nothing() {
    // Given
    let host = a_host();
    let cloner = RecordingCloner::failing_with("no such project on the facilitator");

    // When
    let err = host
        .provision_under_repos_base(&cloner)
        .expect_err("a failed clone must not be masked");

    // Then
    assert_eq!(
        (err.code, host.stored_row().is_none()),
        (Code::Internal, true)
    );
}

#[test]
fn an_unreadable_registry_is_reported_as_internal_without_cloning() {
    // Given
    let host = a_host().with_unreadable_registry();
    let cloner = RecordingCloner::succeeding();

    // When
    let err = host
        .provision_under_repos_base(&cloner)
        .expect_err("a registry that cannot be read must not be treated as empty");

    // Then
    assert_eq!(
        (
            err.code,
            err.message.contains("read project registry"),
            cloner.calls()
        ),
        (Code::Internal, true, vec![])
    );
}

#[test]
fn a_registry_that_cannot_be_written_is_reported_after_the_clone() {
    // Given
    let host = a_host().with_unwritable_registry();
    let cloner = RecordingCloner::succeeding();

    // When
    let err = host
        .provision_under_repos_base(&cloner)
        .expect_err("an unregistered clone must be reported");

    // Then
    assert_eq!(
        (
            err.code,
            err.message.contains("register provisioned project")
        ),
        (Code::Internal, true),
        "message was '{}'",
        err.message
    );
}
