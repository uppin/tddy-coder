//! A project row's home agent in `projects.yaml`: how it is written, cleared and read back.

use std::collections::HashMap;

use tddy_projects::project_storage::{self, ProjectData};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";

fn a_project_row() -> ProjectData {
    ProjectData {
        project_id: ALPHA.to_string(),
        name: "alpha".to_string(),
        git_url: "https://github.com/acme/alpha.git".to_string(),
        main_repo_path: "/repos/alpha".to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
        accounts: Vec::new(),
        local_gh: false,
        home_agent: None,
    }
}

/// A registry holding one project row.
fn a_registry_holding(row: ProjectData) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    project_storage::write_projects(dir.path(), &[row]).expect("seed the registry");
    dir
}

fn stored_row(dir: &std::path::Path) -> ProjectData {
    project_storage::find_project(dir, ALPHA)
        .expect("read registry")
        .expect("project row is present")
}

#[test]
fn a_project_row_written_without_home_agent_reads_as_unset() {
    // Given a row an older daemon wrote, with no home_agent key
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {ALPHA}\n    name: alpha\n    git_url: https://github.com/acme/alpha.git\n    main_repo_path: /repos/alpha\n"
        ),
    )
    .unwrap();

    // When it is read
    let row = stored_row(dir.path());

    // Then it has no home agent
    assert_eq!(row.home_agent, None);
}

#[test]
fn setting_a_home_agent_stores_it_on_the_row() {
    // Given a registry holding a project with no home agent
    let dir = a_registry_holding(a_project_row());

    // When its home agent is set
    project_storage::set_project_home_agent(dir.path(), ALPHA, Some("claude"))
        .expect("setting a home agent succeeds");

    // Then the row carries it
    assert_eq!(stored_row(dir.path()).home_agent.as_deref(), Some("claude"));
}

#[test]
fn clearing_the_home_agent_writes_no_home_agent_key() {
    // Given a registry holding a project whose home agent is set
    let dir = a_registry_holding(ProjectData {
        home_agent: Some("claude".to_string()),
        ..a_project_row()
    });

    // When it is cleared
    project_storage::set_project_home_agent(dir.path(), ALPHA, None)
        .expect("clearing a home agent succeeds");

    // Then the row has none, and the file does not mention it
    assert_eq!(stored_row(dir.path()).home_agent, None);
    let yaml = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();
    assert!(!yaml.contains("home_agent"), "unexpected key in:\n{yaml}");
}

#[test]
fn setting_a_home_agent_on_an_unknown_project_is_an_error() {
    // Given an empty registry
    let dir = tempfile::tempdir().unwrap();
    project_storage::write_projects(dir.path(), &[]).expect("seed the registry");

    // When a home agent is set on a project it does not hold
    let outcome = project_storage::set_project_home_agent(dir.path(), ALPHA, Some("claude"));

    // Then it is refused
    assert!(outcome.is_err());
}
