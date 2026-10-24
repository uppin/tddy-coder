//! `resolve_remote_git_repo_path`: which `tddy-remote-git-repo` the facilitator clone drives as
//! its `GIT_SSH_COMMAND` — an explicit `CARGO_BIN_EXE_tddy-remote-git-repo` override first, then a
//! binary beside the running executable, then a bare name for `PATH` lookup.
//!
//! The function reads process-global state (the environment and `current_exe()`), so every test in
//! this binary holds [`ENV_LOCK`] while it sets the override.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tddy_projects::project_provision::resolve_remote_git_repo_path;

const OVERRIDE_VAR: &str = "CARGO_BIN_EXE_tddy-remote-git-repo";
const BINARY: &str = "tddy-remote-git-repo";

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn the_environment() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Every answer the probe may give once the override is out of the way: a binary in the test
/// executable's bin dir (`target/<profile>/`), one in its own dir (`target/<profile>/deps/`), or
/// the bare name. Which one it is depends on what has been built, not on the code under test.
fn the_answers_without_an_override() -> Vec<String> {
    let exe = std::env::current_exe().expect("current_exe");
    let exe_dir = exe.parent().expect("the test binary has a parent dir");
    let bin_dir = exe_dir.parent().expect("deps/ has a parent dir");
    vec![
        bin_dir.join(BINARY).to_string_lossy().into_owned(),
        exe_dir.join(BINARY).to_string_lossy().into_owned(),
        BINARY.to_string(),
    ]
}

#[test]
fn an_explicit_override_is_returned_as_given() {
    // Given
    let _env = the_environment();
    let explicit = PathBuf::from("/opt/tddy/bin/tddy-remote-git-repo");
    std::env::set_var(OVERRIDE_VAR, &explicit);

    // When
    let resolved = resolve_remote_git_repo_path();

    // Then
    std::env::remove_var(OVERRIDE_VAR);
    assert_eq!(resolved, explicit.to_string_lossy());
}

#[test]
fn a_blank_override_is_ignored() {
    // Given
    let _env = the_environment();
    std::env::set_var(OVERRIDE_VAR, "   ");

    // When
    let resolved = resolve_remote_git_repo_path();

    // Then
    std::env::remove_var(OVERRIDE_VAR);
    assert!(
        the_answers_without_an_override().contains(&resolved),
        "resolved '{resolved}', expected one of {:?}",
        the_answers_without_an_override()
    );
}

#[test]
fn without_an_override_it_resolves_beside_the_executable_or_on_path() {
    // Given
    let _env = the_environment();
    std::env::remove_var(OVERRIDE_VAR);

    // When
    let resolved = resolve_remote_git_repo_path();

    // Then
    assert!(
        the_answers_without_an_override().contains(&resolved),
        "resolved '{resolved}', expected one of {:?}",
        the_answers_without_an_override()
    );
}
