//! Acceptance tests for `web-dev` (PRD: web-dev daemon-only refactor).
//! Static contract checks — no servers. See PRD Testing Plan.
//!
//! Integration layer: delegates to [`tddy_e2e::web_dev_contract`] for static contract checks.

use std::fs;
use std::path::PathBuf;

use tddy_e2e::dev_script_contract::{
    verify_build_flag_uses_shared_runtime_list, verify_declares_and_consumes_new_flags,
    verify_private_repo_flag_delegates_to_local_registry,
};
use tddy_e2e::web_dev_contract::{
    verify_daemon_binary_only, verify_default_dev_daemon_config, verify_syntax_and_no_legacy_branch,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn web_dev_path() -> PathBuf {
    repo_root().join("web-dev")
}

fn read_web_dev() -> String {
    let path = web_dev_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `bash -n` must accept `web-dev`, and the script must stay free of the legacy
/// `USE_DAEMON` / `TDDY_USE_DAEMON` gate (PRD: single flow). (ShellCheck is not run here.)
#[test]
fn web_dev_bash_syntax_and_no_legacy_daemon_gate() {
    // Given
    let path = web_dev_path();

    // When / Then
    verify_syntax_and_no_legacy_branch(&path);
}

/// Resolved backend must be `tddy-daemon` only; no `find_binary` path to `tddy-demo`.
#[test]
fn web_dev_always_targets_tddy_daemon_binary() {
    // Given
    let contents = read_web_dev();

    // When / Then
    verify_daemon_binary_only(&contents);
}

/// With `DAEMON_CONFIG` unset, default config file is `dev.daemon.yaml` at repo root (same as prior daemon branch).
#[test]
fn web_dev_default_config_is_dev_daemon_yaml() {
    // Given
    let contents = read_web_dev();

    // When / Then
    verify_default_dev_daemon_config(&contents);
}

/// `--build` and `--resolve-private-repo` must be documented and consumed here: every unrecognised
/// argument is forwarded to `tddy-daemon`, which would reject or misread them.
#[test]
fn web_dev_declares_build_and_private_repo_flags() {
    // Given
    let contents = read_web_dev();

    // When / Then
    verify_declares_and_consumes_new_flags(&contents, "web-dev");
}

/// `--build` must build the shared runtime-invocable package list, not just the daemon the script
/// launches itself.
#[test]
fn web_dev_build_flag_uses_shared_runtime_list() {
    // Given
    let contents = read_web_dev();

    // When / Then
    verify_build_flag_uses_shared_runtime_list(&contents, "web-dev");
}

/// `--resolve-private-repo` must go through `bun run local-registry-install`.
#[test]
fn web_dev_private_repo_flag_uses_local_registry() {
    // Given
    let contents = read_web_dev();

    // When / Then
    verify_private_repo_flag_delegates_to_local_registry(&contents, "web-dev");
}
