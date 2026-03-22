//! Static contract checks for the repo-root `web-dev` script (daemon-only flow).
//!
//! [`verify_*`] orchestrates `bash -n`, file reads, and granular detectors. Used by integration
//! tests and `#[cfg(test)]` granular tests.

use std::fs;
use std::path::Path;
use std::process::Command;

// --- Granular detectors (reused from [`verify_*]) ---

/// Returns true if the script still wires a `USE_DAEMON` / `TDDY_USE_DAEMON` gate (legacy).
pub fn contains_legacy_daemon_env_gate(contents: &str) -> bool {
    let hit = contents.contains("USE_DAEMON=") || contents.contains("TDDY_USE_DAEMON");
    if hit {
        log::debug!(
            "contains_legacy_daemon_env_gate: legacy USE_DAEMON / TDDY_USE_DAEMON still present"
        );
    }
    hit
}

/// Returns true if the script still references the `tddy-demo` binary or its target paths.
pub fn contains_tddy_demo_binary_paths(contents: &str) -> bool {
    let hit = contents.contains("tddy-demo")
        || contents.contains("target/debug/tddy-demo")
        || contents.contains("target/release/tddy-demo");
    if hit {
        log::debug!("contains_tddy_demo_binary_paths: tddy-demo reference still present");
    }
    hit
}

/// Returns true if the script defaults `CONFIG` to `dev.config.yaml` (legacy demo stack).
pub fn defaults_config_to_dev_config_yaml(contents: &str) -> bool {
    let hit = contents.contains("CONFIG:-dev.config.yaml");
    if hit {
        log::debug!("defaults_config_to_dev_config_yaml: CONFIG:-dev.config.yaml still present");
    }
    hit
}

/// Returns true if the script sets the daemon config default via `DAEMON_CONFIG:-dev.daemon.yaml`.
pub fn has_daemon_config_default_clause(contents: &str) -> bool {
    let ok = contents.contains("DAEMON_CONFIG:-dev.daemon.yaml");
    log::debug!("has_daemon_config_default_clause has_clause={ok}");
    ok
}

// --- Orchestration ---

/// PRD: `bash -n` succeeds and the script does not branch on `USE_DAEMON` / `TDDY_USE_DAEMON`.
pub fn verify_syntax_and_no_legacy_branch(path: &Path) {
    log::info!("verify_syntax_and_no_legacy_branch: start path={path:?}");
    let path_str = path.to_str().expect("web-dev path must be UTF-8");
    let status = Command::new("bash")
        .args(["-n", path_str])
        .status()
        .unwrap_or_else(|e| panic!("spawn bash -n for {path_str}: {e}"));
    log::debug!("bash -n completed status={status:?}");
    assert!(
        status.success(),
        "bash -n web-dev must exit 0 (syntax); got {:?}",
        status.code()
    );

    let contents =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    log::debug!(
        "read web-dev for legacy gate check bytes={}",
        contents.len()
    );
    assert!(
        !contains_legacy_daemon_env_gate(&contents),
        "web-dev must not branch on USE_DAEMON / TDDY_USE_DAEMON (daemon-only flow)"
    );
    log::info!("verify_syntax_and_no_legacy_branch: ok");
}

/// PRD: resolved backend is `tddy-daemon` only; no `tddy-demo` binary paths.
pub fn verify_daemon_binary_only(contents: &str) {
    log::info!("verify_daemon_binary_only: start bytes={}", contents.len());
    assert!(
        contents.contains("tddy-daemon"),
        "web-dev must resolve and run tddy-daemon"
    );
    assert!(
        !contains_tddy_demo_binary_paths(contents),
        "web-dev must not reference tddy-demo binary paths"
    );
    log::info!("verify_daemon_binary_only: ok");
}

/// PRD: default config uses `DAEMON_CONFIG:-dev.daemon.yaml` and does not default `CONFIG` to `dev.config.yaml`.
pub fn verify_default_dev_daemon_config(contents: &str) {
    log::info!(
        "verify_default_dev_daemon_config: start bytes={}",
        contents.len()
    );
    assert!(
        has_daemon_config_default_clause(contents),
        "web-dev must default daemon config via DAEMON_CONFIG:-dev.daemon.yaml when unset"
    );
    assert!(
        !defaults_config_to_dev_config_yaml(contents),
        "web-dev must not default CONFIG to dev.config.yaml for the backend"
    );
    log::info!("verify_default_dev_daemon_config: ok");
}

#[cfg(test)]
mod granular_tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_web_dev_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("web-dev")
    }

    fn read_repo_web_dev() -> String {
        let p = repo_web_dev_path();
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    /// Delegates to [`verify_syntax_and_no_legacy_branch`] (same checks as integration tests).
    #[test]
    fn legacy_daemon_env_gate_absent_including_bash_syntax() {
        verify_syntax_and_no_legacy_branch(&repo_web_dev_path());
    }

    /// Delegates to [`verify_daemon_binary_only`] (same checks as integration tests).
    #[test]
    fn tddy_demo_paths_absent() {
        let contents = read_repo_web_dev();
        verify_daemon_binary_only(&contents);
    }

    /// Delegates to [`verify_default_dev_daemon_config`] (same checks as integration tests).
    #[test]
    fn dev_config_yaml_not_defaulted() {
        let contents = read_repo_web_dev();
        verify_default_dev_daemon_config(&contents);
    }
}
