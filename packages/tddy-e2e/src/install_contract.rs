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
/// INSTALL_WEB_BUNDLE_DIR, INSTALL_SUPERVISOR_SOCKET_PATH, INSTALL_DAEMON_SOCKET_PATH.
pub fn verify_env_override_references(contents: &str) {
    for name in [
        "INSTALL_PREFIX",
        "INSTALL_BIN_DIR",
        "INSTALL_CONFIG_DIR",
        "INSTALL_SYSTEMD_DIR",
        "INSTALL_WEB_BUNDLE_DIR",
        // The supervisor socket path appears in three places install writes (supervisor.yaml,
        // daemon.yaml, tddy-supervisor.socket), so it has to be overridable in one place.
        "INSTALL_SUPERVISOR_SOCKET_PATH",
        "INSTALL_DAEMON_SOCKET_PATH",
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

/// Optional `--user` installs a per-user systemd service: gated flag, `systemctl --user`, and a
/// user-manager `[Install]` target (default.target rather than multi-user.target).
pub fn verify_user_flag_support(contents: &str) {
    assert!(contents.contains("--user"), "install must accept --user");
    assert!(
        contents.contains("want_user"),
        "install must gate on a --user flag (e.g. want_user)"
    );
    assert!(
        contents.contains("systemctl --user"),
        "install --user must drive systemctl with --user"
    );
    assert!(
        contents.contains("WantedBy=default.target"),
        "install --user unit must target default.target (user manager)"
    );
}

/// Optional `--headless` installs without requiring or shipping the tddy-web bundle.
pub fn verify_headless_flag_support(contents: &str) {
    assert!(
        contents.contains("--headless"),
        "install must accept --headless"
    );
    assert!(
        contents.contains("want_headless"),
        "install must gate on a --headless flag (e.g. want_headless)"
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

/// Install copies `codex-acp` from `node_modules` when present; mandatory only when daemon config
/// lists agent id `codex-acp` or `INSTALL_BUNDLE_CODEX_ACP=1`.
pub fn verify_install_conditional_codex_acp_bundling(install_contents: &str) {
    assert!(
        install_contents.contains("node_modules/@zed-industries/codex-acp-"),
        "install must resolve @zed-industries/codex-acp platform package under node_modules"
    );
    assert!(
        install_contents.contains("${BIN_DIR}/codex-acp")
            || install_contents.contains("{BIN_DIR}/codex-acp"),
        "install must install codex-acp into BIN_DIR when bundling"
    );
    assert!(
        install_contents.contains("codex_acp_bundling_mandatory")
            || install_contents.contains("INSTALL_BUNDLE_CODEX_ACP"),
        "install must gate mandatory codex-acp bundling on config or INSTALL_BUNDLE_CODEX_ACP"
    );
}

/// Names of the shell variables holding a path this script generates a file at.
const GENERATED_FILE_VARIABLES: [&str; 5] = [
    "CONFIG_FILE",
    "SUPERVISOR_CONFIG_FILE",
    "UNIT_PATH",
    "UNIT_SOCKET_PATH",
    "APPARMOR_PROFILE_PATH",
];

/// No generated file may be the target of a redirect: `> "$DEST"` truncates `$DEST` *before* the
/// content is produced, so a failure mid-write leaves a 0-byte file — which the deliberate
/// "keep an existing file" guards then preserve on every later install, with no way to repair it.
/// Each one is rendered to a temp file and moved into place instead.
pub fn verify_generated_files_are_never_truncated_in_place(contents: &str) {
    for variable in GENERATED_FILE_VARIABLES {
        for redirect in [format!(">\"${variable}\""), format!("> \"${variable}\"")] {
            assert!(
                !contents.contains(&redirect),
                "install must not redirect onto ${variable} ({redirect} truncates it before the \
                 content exists); render to a temp file and move it into place"
            );
        }
    }
    for helper in ["atomically_write()", "render_template()"] {
        assert!(
            contents.contains(helper),
            "install must generate every file through {helper}"
        );
    }
    assert!(
        contents.contains("mktemp") && contents.contains("mv -f"),
        "install's generated-file helpers must write a temp file and move it into place"
    );
}

/// Placeholder values are data, not `sed` syntax: `#` (the delimiter) aborted the write and `&`
/// expanded to the whole match, so a socket path containing either produced an empty or a wrongly
/// rendered config. Substitution must not go through `sed`.
pub fn verify_placeholder_substitution_does_not_use_sed(contents: &str) {
    assert!(
        !contents.contains("sed -e \"s#__"),
        "install must not substitute placeholders with sed: a `#` in a value ends the expression \
         and an `&` expands to the whole match"
    );
}

/// The AppArmor profile is generated like every other file — an existing one is kept — and a
/// failing `apparmor_parser -r` (no AppArmor LSM on the host) must not abort an otherwise complete
/// install under `set -e`.
pub fn verify_apparmor_profile_is_guarded_and_its_reload_is_not_fatal(contents: &str) {
    assert!(
        contents.contains("if [[ -f \"$APPARMOR_PROFILE_PATH\" ]]"),
        "install must keep an existing AppArmor profile instead of overwriting operator edits"
    );
    let reload = contents
        .lines()
        .find(|line| line.contains("apparmor_parser -r \"$APPARMOR_PROFILE_PATH\""))
        .expect("install must reload the AppArmor profile it wrote");
    let reload = reload.trim_start();
    assert!(
        reload.starts_with("if ") || reload.starts_with("elif "),
        "install must handle a failing apparmor_parser instead of letting set -e abort a \
         half-completed install; got: {reload}"
    );
}

/// `Type=simple` makes `systemctl restart` succeed as soon as fork/exec does, so install must
/// observe the unit as active before reporting success — otherwise a crash-looping supervisor and a
/// healthy one are indistinguishable, and the legacy daemon unit is already disabled by then.
pub fn verify_supervisor_unit_is_verified_active(contents: &str) {
    assert!(
        contents.contains("systemctl is-active"),
        "install must check the supervisor unit's state after starting it"
    );
    assert!(
        contents.contains("verify_unit_running tddy-supervisor \"$UNIT_SETTLE_SAMPLES\""),
        "install must sample tddy-supervisor's state across a settle window before reporting done"
    );
}

/// An inherited `tddy-daemon.socket` keeps listening on the very path the supervisor is about to
/// bind, and `disable` neither disarms a listening socket nor survives a `preset-all`. The socket
/// must be disarmed before the service, and both must be masked, not just disabled.
pub fn verify_legacy_daemon_units_are_masked_socket_first(contents: &str) {
    let declaration = contents
        .lines()
        .find(|line| line.starts_with("LEGACY_DAEMON_UNITS="))
        .expect("install must declare LEGACY_DAEMON_UNITS");
    let socket = declaration.find("tddy-daemon.socket").unwrap_or_else(|| {
        panic!("LEGACY_DAEMON_UNITS must list tddy-daemon.socket: {declaration}")
    });
    let service = declaration.find("tddy-daemon.service").unwrap_or_else(|| {
        panic!("LEGACY_DAEMON_UNITS must list tddy-daemon.service: {declaration}")
    });
    assert!(
        socket < service,
        "tddy-daemon.socket must be disarmed before tddy-daemon.service, or a connect() in between \
         re-activates the legacy daemon: {declaration}"
    );
    assert!(
        contents.contains("systemctl mask \"$unit\""),
        "install must mask the legacy units: disable only drops the WantedBy symlink"
    );
    assert!(
        !contents.contains("systemctl disable --now"),
        "install must stop, disable and mask each legacy unit individually (socket first), not \
         `disable --now` them in declaration order"
    );
}

/// The unit-file heredocs are unquoted so `${...}` expands — which means a backtick or `$(` inside
/// one is executed while the unit is being written, and lands in the file as its output.
pub fn verify_unit_templates_contain_no_command_substitution(contents: &str) {
    for terminator in ["UNIT", "SOCKET"] {
        let body = contents
            .split_once(&format!("<<{terminator}\n"))
            .and_then(|(_, after)| after.split_once(&format!("\n{terminator}\n")))
            .map(|(body, _)| body)
            .unwrap_or_else(|| {
                panic!("install must write its unit file from a <<{terminator} heredoc")
            });
        for substitution in ["`", "$("] {
            assert!(
                !body.contains(substitution),
                "the <<{terminator} heredoc must not contain {substitution}: the heredoc is \
                 unquoted, so it would be executed and its output written into the unit file"
            );
        }
    }
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
    verify_user_flag_support(&contents);
    verify_headless_flag_support(&contents);
    let prod = fs::read_to_string(daemon_yaml_production_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", daemon_yaml_production_path.display()));
    verify_install_deploys_web_static_assets(&contents, &prod);
    verify_install_conditional_codex_acp_bundling(&contents);
    verify_generated_files_are_never_truncated_in_place(&contents);
    verify_placeholder_substitution_does_not_use_sed(&contents);
    verify_apparmor_profile_is_guarded_and_its_reload_is_not_fatal(&contents);
    verify_supervisor_unit_is_verified_active(&contents);
    verify_legacy_daemon_units_are_masked_socket_first(&contents);
    verify_unit_templates_contain_no_command_substitution(&contents);
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
    fn install_user_flag_granular() {
        verify_user_flag_support(&read_repo_install());
    }

    #[test]
    fn install_headless_flag_granular() {
        verify_headless_flag_support(&read_repo_install());
    }

    #[test]
    fn install_deploys_web_bundle_granular() {
        verify_install_deploys_web_static_assets(
            &read_repo_install(),
            &read_repo_daemon_yaml_production(),
        );
    }

    #[test]
    fn install_conditional_codex_acp_granular() {
        verify_install_conditional_codex_acp_bundling(&read_repo_install());
    }

    #[test]
    fn never_truncates_a_generated_file_before_its_content_exists() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_generated_files_are_never_truncated_in_place(&script);
    }

    #[test]
    fn substitutes_placeholders_without_sed_so_a_value_is_never_read_as_syntax() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_placeholder_substitution_does_not_use_sed(&script);
    }

    #[test]
    fn keeps_an_existing_apparmor_profile_and_survives_a_failing_reload() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_apparmor_profile_is_guarded_and_its_reload_is_not_fatal(&script);
    }

    #[test]
    fn refuses_to_report_success_until_the_supervisor_unit_is_observed_active() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_supervisor_unit_is_verified_active(&script);
    }

    #[test]
    fn disarms_the_legacy_daemon_socket_before_its_service_and_masks_both() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_legacy_daemon_units_are_masked_socket_first(&script);
    }

    #[test]
    fn writes_unit_files_without_executing_anything_from_their_templates() {
        // Given / When
        let script = read_repo_install();

        // Then
        verify_unit_templates_contain_no_command_substitution(&script);
    }
}
