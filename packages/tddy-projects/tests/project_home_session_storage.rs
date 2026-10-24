//! A project row's main session (`home_session_id`) in `projects.yaml`: how it is recorded,
//! cleared and read back, and what becomes of the `home_agent` key an older daemon wrote.
//!
//! PRD: docs/ft/web/session-drawer.md

use tddy_projects::project_storage::{self, ProjectData};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";
const MAIN_SESSION: &str = "main-0000-0000-0000-000000000000";

/// A registry file holding one project row, `extra` appended to the row's keys.
fn a_registry_with_alpha(extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {ALPHA}\n    name: alpha\n    git_url: https://github.com/acme/alpha.git\n    main_repo_path: /repos/alpha\n{extra}"
        ),
    )
    .unwrap();
    dir
}

/// A registry file holding one project row as an older daemon wrote it — with a home agent.
fn a_registry_written_with_a_home_agent() -> tempfile::TempDir {
    a_registry_with_alpha("    home_agent: claude\n")
}

/// A registry file holding one project row with no main recorded.
fn a_registry_with_no_main() -> tempfile::TempDir {
    a_registry_with_alpha("")
}

fn stored_row(dir: &std::path::Path) -> ProjectData {
    project_storage::find_project(dir, ALPHA)
        .expect("read registry")
        .expect("project row is present")
}

fn the_registry_file(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("projects.yaml")).unwrap()
}

#[test]
fn an_older_row_with_a_home_agent_reads_as_having_no_home_session() {
    // Given
    let dir = a_registry_written_with_a_home_agent();

    // When
    let row = stored_row(dir.path());

    // Then
    assert_eq!(row.home_session_id, None);
}

#[test]
fn a_stored_home_agent_is_not_written_back() {
    // Given
    let dir = a_registry_written_with_a_home_agent();

    // When the row is next written
    project_storage::set_project_home_session(dir.path(), ALPHA, Some(MAIN_SESSION))
        .expect("recording main succeeds");

    // Then
    let yaml = the_registry_file(dir.path());
    assert!(!yaml.contains("home_agent"), "unexpected key in:\n{yaml}");
}

#[test]
fn recording_a_home_session_stores_its_id() {
    // Given
    let dir = a_registry_with_no_main();

    // When
    project_storage::set_project_home_session(dir.path(), ALPHA, Some(MAIN_SESSION))
        .expect("recording main succeeds");

    // Then
    assert_eq!(
        stored_row(dir.path()).home_session_id.as_deref(),
        Some(MAIN_SESSION)
    );
}

#[test]
fn clearing_the_home_session_writes_no_key() {
    // Given a project whose main is recorded
    let dir = a_registry_with_no_main();
    project_storage::set_project_home_session(dir.path(), ALPHA, Some(MAIN_SESSION))
        .expect("recording main succeeds");

    // When
    project_storage::set_project_home_session(dir.path(), ALPHA, None)
        .expect("clearing main succeeds");

    // Then
    assert_eq!(stored_row(dir.path()).home_session_id, None);
    let yaml = the_registry_file(dir.path());
    assert!(
        !yaml.contains("home_session_id"),
        "unexpected key in:\n{yaml}"
    );
}

#[test]
fn recording_on_an_unknown_project_is_an_error() {
    // Given an empty registry
    let dir = tempfile::tempdir().unwrap();
    project_storage::write_projects(dir.path(), &[]).expect("seed the registry");

    // When
    let outcome = project_storage::set_project_home_session(dir.path(), ALPHA, Some(MAIN_SESSION));

    // Then the error names the project, and the registry is left empty
    let error = outcome
        .expect_err("an unknown project is refused")
        .to_string();
    assert!(error.contains(ALPHA), "names the project: {error}");
    assert!(project_storage::read_projects(dir.path())
        .expect("read registry")
        .is_empty());
}
