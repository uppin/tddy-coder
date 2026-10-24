//! A session start forwarded to a peer carries this host's GitHub source for the project, so the
//! peer acts the same way even if its own copy of the project was never set.

use std::collections::HashMap;

use tddy_projects::project_storage::ProjectData;
use tddy_service::proto::session::{GithubSource, StartSessionRequest};

use crate::start_request_checks::{forwarded_start_request, record_requested_github_source};

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn a_project_row(local_gh: bool) -> ProjectData {
    ProjectData {
        project_id: PROJECT_ID.to_string(),
        name: "alpha".to_string(),
        git_url: "https://github.com/acme/alpha.git".to_string(),
        main_repo_path: "/home/dev/repos/alpha".to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
        accounts: Vec::new(),
        local_gh,
        home_session_id: None,
    }
}

fn a_start_naming(github_source: GithubSource) -> StartSessionRequest {
    StartSessionRequest {
        session_token: "session-token-for-ada".to_string(),
        project_id: PROJECT_ID.to_string(),
        daemon_instance_id: "build-box-2".to_string(),
        github_source: github_source as i32,
        ..Default::default()
    }
}

#[test]
fn a_forwarded_start_of_a_local_gh_project_names_local_gh() {
    // Given a start naming no source, of a project this host knows as Local gh
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(true)));

    // Then the peer is told Local gh
    assert_eq!(forwarded.github_source(), GithubSource::LocalGh);
}

#[test]
fn a_forwarded_start_of_a_vault_project_names_the_vault() {
    // Given a start naming no source, of a project this host knows as using the vault
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(false)));

    // Then the peer is told the vault
    assert_eq!(forwarded.github_source(), GithubSource::Vault);
}

#[test]
fn a_forwarded_start_that_already_names_a_source_keeps_it() {
    // Given a start already naming Local gh, of a project this host knows as using the vault
    let req = a_start_naming(GithubSource::LocalGh);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(false)));

    // Then the source it named is the one forwarded
    assert_eq!(forwarded.github_source(), GithubSource::LocalGh);
}

#[test]
fn a_forwarded_start_of_a_project_this_host_does_not_hold_names_no_source() {
    // Given a start naming no source, of a project this host has no row for
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, None);

    // Then the peer decides from its own row
    assert_eq!(forwarded.github_source(), GithubSource::Unspecified);
}

#[test]
fn a_forwarded_start_is_otherwise_the_request_as_received() {
    // Given a start naming no source, of a Local gh project
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(true)));

    // Then everything but the source is unchanged
    assert_eq!(
        StartSessionRequest {
            github_source: req.github_source,
            ..forwarded
        },
        req
    );
}

#[test]
fn a_start_that_carried_a_source_records_it_in_the_sessions_github_source_file() {
    // Given a start naming Local gh and a session directory that does not exist yet
    let scratch = tempfile::tempdir().unwrap();
    let session_dir = scratch.path().join("session-1");
    let req = a_start_naming(GithubSource::LocalGh);

    // When the start's source is recorded
    record_requested_github_source(&req, &session_dir).unwrap();

    // Then the sidecar file exists and reads back as Local gh
    assert!(session_dir.join("github-source.yaml").is_file());
    assert_eq!(
        tddy_core::read_session_github_source(&session_dir).unwrap(),
        Some(tddy_core::SessionGithubSource::LocalGh)
    );
}

#[test]
fn a_start_that_named_no_source_writes_no_github_source_file() {
    // Given a start naming no source
    let scratch = tempfile::tempdir().unwrap();
    let req = a_start_naming(GithubSource::Unspecified);

    // When the start's source is recorded
    record_requested_github_source(&req, scratch.path()).unwrap();

    // Then nothing is written: resumes read this host's row again
    assert!(!scratch.path().join("github-source.yaml").exists());
}
