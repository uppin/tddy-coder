//! Which account a project uses, as `projects.yaml` stores it.
//!
//! Replace, never merge: two people editing a project's accounts at the same time must not
//! interleave into a set neither of them chose. And a provider appears at most once — a second
//! entry is refused rather than silently winning, because "which GitHub account is this?" has to
//! have one answer.

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_projects::project_storage::{self, AccountAssignment, ProjectData};

const ALPHA: &str = "11111111-2222-4333-8444-555555555555";
const BETA: &str = "66666666-7777-4888-8999-aaaaaaaaaaaa";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_project(project_id: &str) -> ProjectData {
    ProjectData {
        project_id: project_id.to_string(),
        name: "alpha".to_string(),
        git_url: "https://example.com/alpha.git".to_string(),
        main_repo_path: "/repos/alpha".to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
        accounts: Vec::new(),
    }
}

fn an_assignment(provider: &str, account_id: &str) -> AccountAssignment {
    AccountAssignment {
        provider: provider.to_string(),
        account_id: account_id.to_string(),
    }
}

/// The assignment set stored for one project, read back from disk.
fn stored_accounts(dir: &std::path::Path, project_id: &str) -> Vec<AccountAssignment> {
    project_storage::read_projects(dir)
        .expect("read registry")
        .into_iter()
        .find(|p| p.project_id == project_id)
        .expect("project row is present")
        .accounts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn a_row_carries_its_account_assignments_through_a_write_and_a_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = a_project(ALPHA);
    project.accounts = vec![
        an_assignment("github", "ada"),
        an_assignment("cloudflare", "zoe"),
    ];

    project_storage::write_projects(dir.path(), &[project]).expect("write registry");

    assert_eq!(
        stored_accounts(dir.path(), ALPHA),
        vec![
            an_assignment("github", "ada"),
            an_assignment("cloudflare", "zoe")
        ]
    );
}

#[test]
fn a_row_written_by_a_daemon_that_had_no_accounts_field_reads_as_unassigned() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("projects.yaml"),
        format!(
            "projects:\n- project_id: {ALPHA}\n  name: alpha\n  git_url: https://example.com/alpha.git\n  main_repo_path: /repos/alpha\n"
        ),
    )
    .expect("seed a legacy registry");

    let accounts = stored_accounts(dir.path(), ALPHA);

    assert_eq!(accounts, Vec::<AccountAssignment>::new());
}

#[test]
fn an_unassigned_row_writes_no_accounts_key_at_all() {
    let dir = tempfile::tempdir().unwrap();

    project_storage::write_projects(dir.path(), &[a_project(ALPHA)]).expect("write registry");

    let yaml = std::fs::read_to_string(dir.path().join("projects.yaml")).expect("read registry");
    assert_eq!(yaml.contains("accounts"), false);
}

#[test]
fn setting_a_projects_accounts_replaces_the_whole_set_rather_than_merging_into_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = a_project(ALPHA);
    project.accounts = vec![
        an_assignment("github", "ada"),
        an_assignment("cloudflare", "zoe"),
    ];
    project_storage::write_projects(dir.path(), &[project]).expect("seed registry");

    project_storage::set_project_accounts(dir.path(), ALPHA, &[an_assignment("github", "bob")])
        .expect("set accounts succeeds");

    assert_eq!(
        stored_accounts(dir.path(), ALPHA),
        vec![an_assignment("github", "bob")]
    );
}

#[test]
fn setting_an_empty_set_returns_a_project_to_unassigned() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = a_project(ALPHA);
    project.accounts = vec![an_assignment("github", "ada")];
    project_storage::write_projects(dir.path(), &[project]).expect("seed registry");

    project_storage::set_project_accounts(dir.path(), ALPHA, &[]).expect("clear accounts succeeds");

    assert_eq!(
        stored_accounts(dir.path(), ALPHA),
        Vec::<AccountAssignment>::new()
    );
}

#[test]
fn two_accounts_at_one_provider_are_refused_and_the_reason_names_that_provider() {
    let dir = tempfile::tempdir().unwrap();
    project_storage::write_projects(dir.path(), &[a_project(ALPHA)]).expect("seed registry");

    let outcome = project_storage::set_project_accounts(
        dir.path(),
        ALPHA,
        &[
            an_assignment("github", "ada"),
            an_assignment("github", "bob"),
        ],
    );

    assert_eq!(
        outcome.map_err(|e| e.to_string().contains("github")),
        Err(true)
    );
}

#[test]
fn a_refused_assignment_leaves_the_stored_set_exactly_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = a_project(ALPHA);
    project.accounts = vec![an_assignment("github", "ada")];
    project_storage::write_projects(dir.path(), &[project]).expect("seed registry");

    let _refused = project_storage::set_project_accounts(
        dir.path(),
        ALPHA,
        &[
            an_assignment("github", "bob"),
            an_assignment("github", "cid"),
        ],
    );

    assert_eq!(
        stored_accounts(dir.path(), ALPHA),
        vec![an_assignment("github", "ada")]
    );
}

#[test]
fn setting_one_projects_accounts_leaves_every_other_row_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut beta = a_project(BETA);
    beta.accounts = vec![an_assignment("github", "zoe")];
    project_storage::write_projects(dir.path(), &[a_project(ALPHA), beta]).expect("seed registry");

    project_storage::set_project_accounts(dir.path(), ALPHA, &[an_assignment("github", "ada")])
        .expect("set accounts succeeds");

    assert_eq!(
        stored_accounts(dir.path(), BETA),
        vec![an_assignment("github", "zoe")]
    );
}

#[test]
fn setting_accounts_on_a_project_the_registry_does_not_know_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    project_storage::write_projects(dir.path(), &[a_project(ALPHA)]).expect("seed registry");

    let outcome =
        project_storage::set_project_accounts(dir.path(), BETA, &[an_assignment("github", "ada")]);

    assert_eq!(outcome.is_err(), true);
}
