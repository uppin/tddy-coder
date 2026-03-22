//! Acceptance tests for `web-dev` (PRD: web-dev daemon-only refactor).
//! Static contract checks — no servers. See PRD Testing Plan.
//!
//! Integration layer: delegates to [`tddy_e2e::web_dev_contract`] for static contract checks.

use std::fs;
use std::path::PathBuf;

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
    verify_syntax_and_no_legacy_branch(&web_dev_path());
}

/// Resolved backend must be `tddy-daemon` only; no `find_binary` path to `tddy-demo`.
#[test]
fn web_dev_always_targets_tddy_daemon_binary() {
    let contents = read_web_dev();
    verify_daemon_binary_only(&contents);
}

/// With `DAEMON_CONFIG` unset, default config file is `dev.daemon.yaml` at repo root (same as prior daemon branch).
#[test]
fn web_dev_default_config_is_dev_daemon_yaml() {
    let contents = read_web_dev();
    verify_default_dev_daemon_config(&contents);
}
