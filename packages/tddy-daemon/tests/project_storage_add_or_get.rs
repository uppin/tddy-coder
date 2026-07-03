//! Unit: `project_storage::add_or_get_project` appends a row only when the `project_id` is absent,
//! and otherwise returns the existing row untouched — the idempotency primitive behind
//! `AddProjectToHost` (PRD docs/ft/web/projects-screen-multi-host.md).

use std::collections::HashMap;

use tddy_daemon::project_storage::{self, ProjectData};

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn a_project(name: &str, repo_path: &str) -> ProjectData {
    ProjectData {
        project_id: PROJECT_ID.to_string(),
        name: name.to_string(),
        git_url: "https://example.com/alpha.git".to_string(),
        main_repo_path: repo_path.to_string(),
        main_branch_ref: None,
        host_repo_paths: HashMap::new(),
    }
}

#[test]
fn add_or_get_project_appends_and_reports_created_when_the_id_is_absent() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    let (stored, created) =
        project_storage::add_or_get_project(dir.path(), a_project("alpha", "/repos/alpha"))
            .expect("add_or_get_project succeeds");

    // Then
    assert!(created, "a brand-new project_id must report created = true");
    assert_eq!(stored.project_id, PROJECT_ID);
    let rows = project_storage::read_projects(dir.path()).expect("read registry");
    assert_eq!(rows.len(), 1);
}

#[test]
fn add_or_get_project_returns_the_existing_row_without_duplicating_when_the_id_is_present() {
    // Given — the id already exists with its original name/path
    let dir = tempfile::tempdir().unwrap();
    project_storage::add_project(dir.path(), a_project("alpha", "/repos/alpha"))
        .expect("seed existing project");

    // When — a second add with the same id but different fields
    let (stored, created) =
        project_storage::add_or_get_project(dir.path(), a_project("alpha-renamed", "/repos/other"))
            .expect("add_or_get_project succeeds");

    // Then — the original row is returned unchanged and not duplicated
    assert!(
        !created,
        "an existing project_id must report created = false"
    );
    assert_eq!(
        stored.name, "alpha",
        "existing row must be returned, not overwritten"
    );
    assert_eq!(stored.main_repo_path, "/repos/alpha");
    let rows = project_storage::read_projects(dir.path()).expect("read registry");
    assert_eq!(
        rows.len(),
        1,
        "add_or_get_project must not duplicate an existing id"
    );
}
