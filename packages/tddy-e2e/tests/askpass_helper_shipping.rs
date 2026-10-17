//! Acceptance: `tddy-askpass` is shipped and built everywhere its sibling helpers are.
//!
//! The daemon resolves the helper as a sibling of its own executable and then as a bare name on
//! `PATH`, with no existence check — so a list that forgot it is not a build error, it is a clone
//! that cannot ask for a passphrase. `dev_runtime_binaries.rs` already checks that the dev list
//! agrees with `install`; that passes just as well when *neither* names the helper, which is why
//! the helper is pinned by name here.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC14)

use std::path::PathBuf;

use tddy_e2e::dev_script_contract::{read_repo_file, RUNTIME_BINARIES_SCRIPT};

const HELPER: &str = "tddy-askpass";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn the_script(name: &str) -> String {
    read_repo_file(&repo_root(), name)
}

/// The line of `script` that declares `list`, e.g. `DESKTOP_BINARIES=(...)`.
fn the_declaration_of<'a>(script: &'a str, list: &str) -> &'a str {
    script
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{list}=(")))
        .unwrap_or_else(|| panic!("no `{list}=(…)` line found"))
}

#[test]
fn install_ships_the_helper_beside_the_daemon_in_both_deployments() {
    // Given the install script
    let install = the_script("install");

    // Then the systemd deployment and the desktop deployment both ship it
    assert!(
        the_declaration_of(&install, "INSTALLED_BINARIES").contains(HELPER),
        "`./install --systemd` does not ship {HELPER}"
    );
    assert!(
        the_declaration_of(&install, "DESKTOP_BINARIES").contains(HELPER),
        "`./install --desktop` does not ship {HELPER}"
    );
}

#[test]
fn release_builds_the_helper() {
    assert!(
        the_script("release").contains(&format!("-p {HELPER}")),
        "`./release` does not build {HELPER}, so there is nothing for install to ship"
    );
}

#[test]
fn publish_packages_the_helper_into_the_deb() {
    let publish = the_script("publish.sh");
    assert!(
        publish.contains(&format!("target/release/{HELPER}")),
        "`./publish.sh` neither checks for nor stages {HELPER}"
    );
    assert!(
        publish.contains(&format!("/usr/bin/{HELPER}")),
        "`./publish.sh` does not install {HELPER} to /usr/bin"
    );
}

#[test]
fn a_dev_run_built_with_the_build_flag_has_the_helper() {
    assert!(
        the_script(RUNTIME_BINARIES_SCRIPT).contains(HELPER),
        "`web-dev --build` and `desktop-dev --build` would not build {HELPER}"
    );
}

#[test]
fn the_test_runner_builds_the_helper_before_the_suites_that_spawn_it() {
    // The clone acceptance suite lives in a package that does not own the helper, so cargo builds
    // it for no one — `./test` has to, exactly as it does for `tddy-sandbox-runner`.
    assert!(
        the_script("test").contains(&format!("-p {HELPER}")),
        "`./test` does not build {HELPER}; the clone acceptance suite would run against a stale or missing helper"
    );
}
