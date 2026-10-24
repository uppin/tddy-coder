//! A project row's Local gh setting in `projects.yaml`: how it is written, read back, and refused.

use std::collections::HashMap;

use tddy_projects::project_storage::{self, AccountAssignment, ProjectData};

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
        home_session_id: None,
    }
}

fn an_assignment(provider: &str, account_id: &str) -> AccountAssignment {
    AccountAssignment {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
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
fn a_row_written_before_local_gh_existed_reads_as_not_using_it() {
    // Given a row an older daemon wrote, with no local_gh key
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

    // Then it does not use local gh
    assert!(!row.local_gh);
}

#[test]
fn a_row_not_using_local_gh_writes_no_local_gh_key() {
    // Given a registry holding a row that does not use local gh
    let dir = a_registry_holding(a_project_row());

    // When the file is read as text
    let yaml = std::fs::read_to_string(dir.path().join("projects.yaml")).unwrap();

    // Then no local_gh key was written
    assert!(!yaml.contains("local_gh"));
}

#[test]
fn setting_local_gh_stores_it() {
    // Given a project that uses the vault
    let dir = a_registry_holding(a_project_row());

    // When it is set to local gh
    project_storage::set_project_accounts(dir.path(), ALPHA, &[], true)
        .expect("the setting is accepted");

    // Then the row uses local gh
    assert!(stored_row(dir.path()).local_gh);
}

#[test]
fn setting_local_gh_keeps_the_other_providers_assignments_it_was_sent() {
    // Given a project that uses the vault
    let dir = a_registry_holding(a_project_row());

    // When it is set to local gh with a Cloudflare assignment
    project_storage::set_project_accounts(
        dir.path(),
        ALPHA,
        &[an_assignment("cloudflare", "acct-edge")],
        true,
    )
    .expect("the setting is accepted");

    // Then the row holds both
    let row = stored_row(dir.path());
    assert_eq!(
        (row.local_gh, row.accounts),
        (true, vec![an_assignment("cloudflare", "acct-edge")])
    );
}

#[test]
fn local_gh_with_a_github_assignment_is_refused_naming_both() {
    // Given a project that uses the vault
    let dir = a_registry_holding(a_project_row());

    // When local gh is set together with a GitHub account
    let refusal = project_storage::set_project_accounts(
        dir.path(),
        ALPHA,
        &[an_assignment("github", "acct-octocat")],
        true,
    )
    .expect_err("a project has one GitHub source");

    // Then the refusal names both sources
    let message = refusal.to_string();
    assert!(
        message.contains("local gh") && message.contains("github account"),
        "got: {message}"
    );
}

#[test]
fn a_refused_local_gh_setting_leaves_the_row_exactly_as_it_was() {
    // Given a project assigned a GitHub account
    let mut row = a_project_row();
    row.accounts = vec![an_assignment("github", "acct-octocat")];
    let dir = a_registry_holding(row.clone());

    // When local gh is set while still naming a GitHub account
    let _refused = project_storage::set_project_accounts(
        dir.path(),
        ALPHA,
        &[an_assignment("github", "acct-hubot")],
        true,
    );

    // Then the row is untouched
    assert_eq!(stored_row(dir.path()), row);
}
