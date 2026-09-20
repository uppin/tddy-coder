//! The login-time GitHub token store, and the readers `#keyring` 9/9 takes off it.
//!
//! `FileGitHubTokenStore` is the shape this whole stack replaces: one file of `login -> token`,
//! written at login, read by whatever part of the daemon needed *a* GitHub credential. It answers
//! the wrong question — *which operator signed in*, not *which account this project acts as* — so
//! a daemon holding two accounts could only ever act as one of them, and a project could not
//! choose.
//!
//! These read source text, because "nothing reaches for this any more" is not a question the type
//! system answers while the type still exists.

use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

/// Every `.rs` file under a package's `src/`, as one string.
fn production_source_of(package_name: &str) -> String {
    let mut sources = Vec::new();
    let mut pending = vec![package(package_name).join("src")];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}"));
        for entry in entries.flatten() {
            let path = entry.path();
            pending.extend(path.is_dir().then(|| path.clone()));
            sources.extend(
                (path.extension().is_some_and(|e| e == "rs"))
                    .then(|| std::fs::read_to_string(&path).unwrap_or_default()),
            );
        }
    }
    sources.join("\n")
}

#[test]
fn the_session_path_no_longer_reaches_for_the_operator_s_login_time_github_token() {
    // Given the crate that starts a coding session
    let source = production_source_of("tddy-session-lifecycle");

    // Then it names no login-time token store
    assert!(
        !source.contains("GitHubTokenStore"),
        "`tddy-session-lifecycle` still resolves a GitHub credential from the login, not from the \
         project's assigned account"
    );
}

#[test]
fn the_authentication_service_no_longer_retains_a_github_token_beside_the_vault() {
    // Given the crate that mints sessions
    let source = production_source_of("tddy-daemon-auth");

    // Then the file-backed token store is gone from it
    assert!(
        !source.contains("FileGitHubTokenStore"),
        "`tddy-daemon-auth` still writes `github-tokens.json` beside the vault that replaced it"
    );
}

#[test]
fn nothing_in_the_tree_still_declares_a_login_keyed_github_token_store() {
    // Given the crate the trait itself lives in
    let source = production_source_of("tddy-github");

    // Then the trait is gone, not merely unused
    assert!(
        !source.contains("trait GitHubTokenStore"),
        "`GitHubTokenStore` is still declared — a login-keyed store cannot express two accounts"
    );
}

#[test]
fn no_crate_still_resolves_a_github_token_from_the_process_environment() {
    // Given every crate that reaches GitHub over REST
    let source = format!(
        "{}{}",
        production_source_of("tddy-github"),
        production_source_of("tddy-workflow-recipes")
    );

    // Then none of them reads the credential out of the environment it happens to run in
    assert!(
        !source.contains("github_token_from_env"),
        "a GitHub token is still read from the process environment — which account that is depends \
         on who exported the variable, not on what the project was assigned"
    );
}
