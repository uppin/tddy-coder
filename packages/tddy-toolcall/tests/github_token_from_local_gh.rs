//! A Local gh session reads its GitHub token from the host's own `gh`, in the asking process, for
//! the GitHub host its project lives on.
//!
//! `gh` here is a `/bin/sh` stub that answers only the exact argv a real `gh` logged in to one host
//! would, so asking about the wrong host, or the wrong way, gets a logged-out answer rather than a
//! silently correct one.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-14-local-gh-github-source.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use tddy_toolcall::toolcall::{request_github_token_from, SessionTokenSource};

/// A `gh` logged in to `hostname`, answering `auth token --hostname <hostname>` with `token`.
fn a_gh_logged_in_to(dir: &Path, hostname: &str, token: &str) -> PathBuf {
    a_gh_script(
        dir,
        &format!(
            "if [ \"$*\" = \"auth token --hostname {hostname}\" ]; then printf '%s\\n' '{token}'; exit 0; fi\n\
             printf 'no oauth token found for %s\\n' \"$4\" >&2\nexit 1\n"
        ),
    )
}

/// A `gh` logged in to nothing.
fn a_logged_out_gh(dir: &Path) -> PathBuf {
    a_gh_script(dir, "printf 'no oauth token found for %s\\n' \"$4\" >&2\nexit 1\n")
}

fn a_gh_script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("gh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn local_gh_for(hostname: &str, gh: PathBuf) -> SessionTokenSource {
    SessionTokenSource::LocalGh {
        hostname: hostname.to_string(),
        gh_program: Some(gh),
    }
}

#[tokio::test]
async fn a_session_told_local_gh_reads_its_token_from_gh_for_that_host() {
    // Given a host whose gh is logged in to an Enterprise host
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_logged_in_to(dir.path(), "github.example.com", "gho_grace_on_enterprise");

    // When a session whose project lives on that host asks for its token
    let token = request_github_token_from(&local_gh_for("github.example.com", gh)).await;

    // Then it is the token gh holds for that host
    assert_eq!(token, Ok("gho_grace_on_enterprise".to_string()));
}

#[tokio::test]
async fn a_session_whose_project_lives_on_another_host_gets_no_token_from_gh() {
    // Given a host whose gh is logged in to github.com only
    let dir = tempfile::tempdir().unwrap();
    let gh = a_gh_logged_in_to(dir.path(), "github.com", "gho_ada_on_github_com");

    // When a session whose project lives on an Enterprise host asks for its token
    let token = request_github_token_from(&local_gh_for("github.example.com", gh)).await;

    // Then it is refused, naming the host gh was asked about, not handed github.com's token
    let refusal = token.expect_err("gh holds no token for that host");
    assert!(
        refusal.contains("github.example.com"),
        "the refusal must name the host gh was asked about, got: {refusal}"
    );
}

#[tokio::test]
async fn gh_that_is_logged_out_is_reported_in_its_own_words() {
    // Given a host whose gh is logged in to nothing
    let dir = tempfile::tempdir().unwrap();
    let gh = a_logged_out_gh(dir.path());

    // When a Local gh session asks for its token
    let token = request_github_token_from(&local_gh_for("github.com", gh)).await;

    // Then gh's own explanation reaches the asker
    let refusal = token.expect_err("a logged-out gh holds no token");
    assert!(
        refusal.contains("no oauth token found for github.com"),
        "gh's own words must reach the asker, got: {refusal}"
    );
}

#[tokio::test]
async fn a_host_with_no_gh_installed_is_reported_as_such() {
    // Given a Local gh session on a host with no gh on its PATH
    let source = SessionTokenSource::LocalGh {
        hostname: "github.com".to_string(),
        gh_program: None,
    };

    // When it asks for its token
    let token = request_github_token_from(&source).await;

    // Then it is told gh is not installed, and how to fix it
    let refusal = token.expect_err("there is no gh to ask");
    assert!(
        refusal.contains("gh is not installed") && refusal.contains("gh auth login"),
        "the refusal must say gh is missing and how to log in, got: {refusal}"
    );
}
