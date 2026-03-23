//! Static contract checks for the repo-root `install` script (systemd install flow).

use std::fs;
use std::path::Path;
use std::process::Command;

/// `bash -n` must accept the install script.
pub fn verify_syntax(path: &Path) {
    let path_str = path.to_str().expect("install path must be UTF-8");
    let status = Command::new("bash")
        .args(["-n", path_str])
        .status()
        .unwrap_or_else(|e| panic!("spawn bash -n for {path_str}: {e}"));
    assert!(
        status.success(),
        "bash -n install must exit 0 (syntax); got {:?}",
        status.code()
    );
}

/// Script must document and parse `--systemd` and reject invocation without it.
pub fn verify_requires_systemd_flag(contents: &str) {
    assert!(
        contents.contains("--systemd"),
        "install must reference --systemd"
    );
    assert!(
        contents.contains("want_systemd"),
        "install must gate on a --systemd flag (e.g. want_systemd)"
    );
    assert!(
        contents.contains("Usage: $0 --systemd") || contents.contains("Usage: ${0} --systemd"),
        "install usage must mention --systemd"
    );
}

/// Script must honor INSTALL_PREFIX, INSTALL_BIN_DIR, INSTALL_CONFIG_DIR, INSTALL_SYSTEMD_DIR.
pub fn verify_env_override_references(contents: &str) {
    for name in [
        "INSTALL_PREFIX",
        "INSTALL_BIN_DIR",
        "INSTALL_CONFIG_DIR",
        "INSTALL_SYSTEMD_DIR",
    ] {
        assert!(
            contents.contains(name),
            "install must reference {name} for path overrides"
        );
    }
}

/// Production installs require root unless testing mode is enabled.
pub fn verify_root_check(contents: &str) {
    assert!(
        contents.contains("id -u"),
        "install must check root via id -u"
    );
}

/// Testing / CI skip for systemctl and root.
pub fn verify_no_systemctl_support(contents: &str) {
    assert!(
        contents.contains("INSTALL_NO_SYSTEMCTL"),
        "install must support INSTALL_NO_SYSTEMCTL"
    );
}

/// Optional `--build` runs the release script.
pub fn verify_build_flag_invokes_release(contents: &str) {
    assert!(contents.contains("--build"), "install must accept --build");
    assert!(
        contents.contains("/release") || contents.contains("./release"),
        "install --build must invoke ./release"
    );
}

/// Orchestration: syntax + static contracts (used by integration tests).
pub fn verify_install_script_contracts(path: &Path) {
    verify_syntax(path);
    let contents =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    verify_requires_systemd_flag(&contents);
    verify_env_override_references(&contents);
    verify_root_check(&contents);
    verify_no_systemctl_support(&contents);
    verify_build_flag_invokes_release(&contents);
}

#[cfg(test)]
mod granular_tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_install_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("install")
    }

    fn read_repo_install() -> String {
        let p = repo_install_path();
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    #[test]
    fn install_bash_syntax_granular() {
        verify_syntax(&repo_install_path());
    }

    #[test]
    fn install_requires_systemd_flag_granular() {
        verify_requires_systemd_flag(&read_repo_install());
    }

    #[test]
    fn install_env_overrides_granular() {
        verify_env_override_references(&read_repo_install());
    }

    #[test]
    fn install_root_check_granular() {
        verify_root_check(&read_repo_install());
    }

    #[test]
    fn install_no_systemctl_granular() {
        verify_no_systemctl_support(&read_repo_install());
    }

    #[test]
    fn install_build_flag_granular() {
        verify_build_flag_invokes_release(&read_repo_install());
    }
}
