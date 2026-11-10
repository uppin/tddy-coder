//! Acceptance: `tddy-git-clone`, the clone helper a supervised daemon runs as the session user
//! (`#agent-tools` 32/32), is built and shipped beside the daemon everywhere its sibling helpers are.
//!
//! The daemon resolves it as a sibling of its own executable, so a list that forgot it is not a
//! build error — it is a supervised host that cannot clone. Not in `./install --desktop`: Tddy
//! Desktop runs no supervisor.
//!
//! Feature: docs/ft/supervisor/1-WIP/PRD-2026-11-07-supervisor-agent-agnostic.md

use std::path::PathBuf;

use tddy_e2e::dev_script_contract::{read_repo_file, RUNTIME_BINARIES_SCRIPT};

const HELPER: &str = "tddy-git-clone";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn the_script(name: &str) -> String {
    read_repo_file(&repo_root(), name)
}

#[test]
fn install_ships_the_clone_helper_beside_the_daemon() {
    let install = the_script("install");
    let installed = install
        .lines()
        .find(|line| line.trim_start().starts_with("INSTALLED_BINARIES=("))
        .expect("an `INSTALLED_BINARIES=(…)` line");
    assert!(
        installed.contains(HELPER),
        "`./install --systemd` does not ship {HELPER} beside tddy-daemon"
    );
}

#[test]
fn release_builds_the_clone_helper() {
    assert!(
        the_script("release").contains(&format!("-p {HELPER}")),
        "`./release` does not build {HELPER}, so there is nothing for install to ship"
    );
}

#[test]
fn publish_packages_the_clone_helper_into_the_deb() {
    let publish = the_script("publish.sh");
    assert!(
        publish.contains(&format!("target/release/{HELPER}"))
            && publish.contains(&format!("/usr/bin/{HELPER}")),
        "`./publish.sh` does not stage {HELPER} into /usr/bin"
    );
}

#[test]
fn a_dev_run_built_with_the_build_flag_has_the_clone_helper() {
    assert!(
        the_script(RUNTIME_BINARIES_SCRIPT).contains(HELPER),
        "`web-dev --build` and `desktop-dev --build` would not build {HELPER}"
    );
}
