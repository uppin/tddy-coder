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

/// Script must honor INSTALL_PREFIX, INSTALL_BIN_DIR, INSTALL_CONFIG_DIR, INSTALL_SYSTEMD_DIR,
/// INSTALL_WEB_BUNDLE_DIR.
pub fn verify_env_override_references(contents: &str) {
    for name in [
        "INSTALL_PREFIX",
        "INSTALL_BIN_DIR",
        "INSTALL_CONFIG_DIR",
        "INSTALL_SYSTEMD_DIR",
        "INSTALL_WEB_BUNDLE_DIR",
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

/// Optional overwrite of systemd unit (preserve User= / manual edits by default).
pub fn verify_install_overwrite_systemd_unit(contents: &str) {
    assert!(
        contents.contains("INSTALL_OVERWRITE_SYSTEMD_UNIT"),
        "install must document INSTALL_OVERWRITE_SYSTEMD_UNIT for replacing the unit file"
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

/// `daemon.yaml.production` sets `web_bundle_path`; install must deploy `packages/tddy-web/dist`
/// there so the path exists after install (otherwise the daemon serves a missing directory).
pub fn verify_install_deploys_web_static_assets(
    install_contents: &str,
    daemon_yaml_production: &str,
) {
    let bundle_decl = daemon_yaml_production
        .lines()
        .find(|l| l.trim_start().starts_with("web_bundle_path:"))
        .expect("daemon.yaml.production must declare web_bundle_path");
    assert!(
        install_contents.contains("packages/tddy-web/dist")
            || install_contents.contains("tddy-web/dist"),
        "install must copy the built tddy-web bundle into web_bundle_path ({bundle_decl}); \
         otherwise that path is missing on disk and the daemon cannot serve static files"
    );
}

/// Orchestration: syntax + static contracts (used by integration tests).
pub fn verify_install_script_contracts(path: &Path, daemon_yaml_production_path: &Path) {
    verify_syntax(path);
    let contents =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    verify_requires_systemd_flag(&contents);
    verify_env_override_references(&contents);
    verify_root_check(&contents);
    verify_no_systemctl_support(&contents);
    verify_install_overwrite_systemd_unit(&contents);
    verify_build_flag_invokes_release(&contents);
    let prod = fs::read_to_string(daemon_yaml_production_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", daemon_yaml_production_path.display()));
    verify_install_deploys_web_static_assets(&contents, &prod);
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

    fn repo_daemon_yaml_production_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("daemon.yaml.production")
    }

    fn read_repo_daemon_yaml_production() -> String {
        let p = repo_daemon_yaml_production_path();
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
    fn install_overwrite_systemd_unit_granular() {
        verify_install_overwrite_systemd_unit(&read_repo_install());
    }

    #[test]
    fn install_build_flag_granular() {
        verify_build_flag_invokes_release(&read_repo_install());
    }

    #[test]
    fn install_deploys_web_bundle_granular() {
        verify_install_deploys_web_static_assets(
            &read_repo_install(),
            &read_repo_daemon_yaml_production(),
        );
    }
}
