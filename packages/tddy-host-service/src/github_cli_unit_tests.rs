//! Asking the host's `gh`, as an OS user, who it is logged in as for one GitHub host, and its token.
//!
//! `gh` is a stub answering only the exact argv a real `gh` logged in to one host would
//! (`tddy_testing_commons::gh_cli`). Every question is asked as the OS user running the suite — the
//! one account a test can start a child as without privilege.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use tddy_testing_commons::gh_cli::a_gh_cli;

use crate::github_cli::{GhUser, GithubCli, GithubCliRefusal};

fn the_current_os_user() -> String {
    std::env::var("USER").expect("USER must be set to run this test")
}

/// A `gh` that answers `api user` with output no `gh` release prints.
fn a_gh_answering_something_unrecognised(dir: &Path) -> PathBuf {
    let path = dir.join("gh");
    std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' 'Welcome to gh!'\nexit 0\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_logged_in_user_is_who_gh_answers_for_that_host() {
    // Given gh logged in to an Enterprise host as grace, GitHub user 202
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_cli(dir.path())
        .at_host("github.example.com")
        .logged_in_as("grace", 202)
        .build();

    // When it is asked who it is logged in as there
    let user = GithubCli::at(gh).logged_in_user(&the_current_os_user(), "github.example.com");

    // Then it is grace
    assert_eq!(
        user,
        Ok(GhUser {
            login: "grace".to_string(),
            id: 202,
        })
    );
}

#[test]
fn a_gh_logged_out_for_that_host_is_refused_as_not_logged_in() {
    // Given gh logged in to github.com only
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_cli(dir.path()).logged_in_as("ada", 101).build();
    let os_user = the_current_os_user();

    // When it is asked about an Enterprise host
    let refusal = GithubCli::at(gh).logged_in_user(&os_user, "github.example.com");

    // Then it is not logged in there, for that user
    assert_eq!(
        refusal,
        Err(GithubCliRefusal::NotLoggedIn {
            hostname: "github.example.com".to_string(),
            os_user,
        })
    );
}

#[test]
fn not_being_logged_in_is_explained_naming_the_host_the_user_and_the_command() {
    // Given a refusal for ada on an Enterprise host
    let refusal = GithubCliRefusal::NotLoggedIn {
        hostname: "github.example.com".to_string(),
        os_user: "ada".to_string(),
    };

    // When it is shown
    let message = refusal.to_string();

    // Then it says where, as whom, and what to run
    assert!(
        message.contains("github.example.com")
            && message.contains("ada")
            && message.contains("gh auth login --hostname github.example.com"),
        "got: {message}"
    );
}

#[test]
fn a_missing_gh_is_refused_as_not_installed() {
    // Given a gh path nothing is installed at
    let dir = tempfile::tempdir().unwrap();
    let gh = dir.path().join("gh");

    // When it is asked who it is logged in as
    let refusal = GithubCli::at(gh).logged_in_user(&the_current_os_user(), "github.com");

    // Then gh is not installed
    assert_eq!(refusal, Err(GithubCliRefusal::NotInstalled));
}

#[test]
fn the_token_is_what_gh_answers_for_that_host() {
    // Given gh logged in to github.com, holding a token
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_cli(dir.path())
        .logged_in_as("ada", 101)
        .with_token("gho_ada_from_the_hosts_gh")
        .build();

    // When it is asked for its token there
    let token = GithubCli::at(gh).auth_token(&the_current_os_user(), "github.com");

    // Then it is that token, without its trailing newline
    assert_eq!(token, Ok("gho_ada_from_the_hosts_gh".to_string()));
}

#[test]
fn gh_answering_something_unrecognised_is_a_failure_not_a_login() {
    // Given a gh that answers `api user` with something no gh prints
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_answering_something_unrecognised(dir.path());

    // When it is asked who it is logged in as
    let refusal = GithubCli::at(gh).logged_in_user(&the_current_os_user(), "github.com");

    // Then it is a failure carrying what gh said — never a login made up from it
    assert!(
        matches!(&refusal, Err(GithubCliRefusal::Failed(reason)) if reason.contains("Welcome to gh!")),
        "got: {refusal:?}"
    );
}
