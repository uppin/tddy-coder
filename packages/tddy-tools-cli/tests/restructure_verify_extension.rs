//! `tddy-tools restructure verify` hands the changed files of an installed extension language to
//! that language's verify and shows what it finds (`#reshape` extension-code-actions, R5).
//!
//! The cold command starts no language server for a verify, so what it judges by is the grammar:
//! the fixture is `tddy-zed-extensions`' `demo-toml` with its vendored TOML grammar, installed
//! under a data dir the child process is pointed at through `TDDY_DATA_DIR`.

use std::fs;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tddy_zed_extensions::testkit::{self, APP, APP_TOML};

/// A tddy data dir with `demo-toml` installed: the fixture extension plus the vendored grammar.
fn a_data_dir_with_demo_toml_installed() -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("a temporary data dir");
    testkit::install_demo_toml(data.path());
    data
}

/// A committed git worktree holding [`APP`] as [`APP_TOML`].
fn a_committed_worktree_holding_the_app_config() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    testkit::commit_baseline(workspace.path(), &[(APP, APP_TOML)]);
    workspace
}

/// `restructure verify --against HEAD` run cold in `workspace`, with `data` as the tddy data dir.
fn verifying_cold(workspace: &Path, data: &Path) -> assert_cmd::assert::Assert {
    let mut command = cargo_bin_cmd!("tddy-tools");
    command
        .env_remove("TDDY_SOCKET")
        .env_remove("TDDY_INDEX_SOCKET")
        .env("TDDY_DATA_DIR", data)
        .current_dir(workspace)
        .args(["restructure", "verify", "--against", "HEAD"])
        .assert()
}

#[test]
fn a_parse_error_in_a_changed_extension_file_is_shown_and_fails_the_verify() {
    // Given a committed TOML file changed to text its grammar cannot parse
    let data = a_data_dir_with_demo_toml_installed();
    let workspace = a_committed_worktree_holding_the_app_config();
    fs::write(workspace.path().join(APP), "[server\nhost = \n").expect("break the app config");

    // When the tree is verified cold against its baseline
    let verified = verifying_cold(workspace.path(), data.path());

    // Then the parse error is shown with where it is, and the run fails
    verified
        .failure()
        .stdout(predicate::str::contains("finding: config/app.toml:1:"))
        .stdout(predicate::str::contains("parse error"));
}

#[test]
fn a_clean_change_to_an_extension_file_shows_no_finding_and_holds() {
    // Given a committed TOML file changed to another valid value
    let data = a_data_dir_with_demo_toml_installed();
    let workspace = a_committed_worktree_holding_the_app_config();
    fs::write(workspace.path().join(APP), APP_TOML.replace("8080", "9090"))
        .expect("change the app config");

    // When the tree is verified cold against its baseline
    let verified = verifying_cold(workspace.path(), data.path());

    // Then the run holds and shows no finding
    verified
        .success()
        .stdout(predicate::str::contains("finding:").not());
}
