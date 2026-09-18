//! Contract and functional tests for repo-root `install` (systemd install flow).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_e2e::install_contract::{
    verify_build_flag_invokes_release, verify_desktop_config_template, verify_desktop_flag_support,
    verify_env_override_references, verify_headless_flag_support, verify_install_script_contracts,
    verify_no_systemctl_support, verify_requires_systemd_flag, verify_root_check, verify_syntax,
    verify_update_systemd_unit_flag, verify_user_flag_support,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn install_path() -> PathBuf {
    repo_root().join("install")
}

fn daemon_yaml_production_path() -> PathBuf {
    repo_root().join("daemon.yaml.production")
}

fn read_install() -> String {
    let path = install_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `bash -n` must accept `install`.
#[test]
fn install_bash_syntax() {
    // Given
    let path = install_path();

    // When / Then
    verify_syntax(&path);
}

#[test]
fn install_requires_systemd_flag() {
    // Given
    let script = read_install();

    // When / Then
    verify_requires_systemd_flag(&script);
}

#[test]
fn install_respects_env_overrides() {
    // Given
    let script = read_install();

    // When / Then
    verify_env_override_references(&script);
}

#[test]
fn install_has_root_check() {
    // Given
    let script = read_install();

    // When / Then
    verify_root_check(&script);
}

#[test]
fn install_update_systemd_unit_flag_documented() {
    // Given
    let script = read_install();

    // When / Then
    verify_update_systemd_unit_flag(&script);
}

#[test]
fn install_no_systemctl_support() {
    // Given
    let script = read_install();

    // When / Then
    verify_no_systemctl_support(&script);
}

#[test]
fn install_build_flag_accepted() {
    // Given
    let script = read_install();

    // When / Then
    verify_build_flag_invokes_release(&script);
}

#[test]
fn install_full_contract_orchestration() {
    // Given
    let path = install_path();
    let yaml = daemon_yaml_production_path();

    // When / Then
    verify_install_script_contracts(&path, &yaml);
}

fn copy_install_tree(dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    let src_install = install_path();
    let dst_install = dest.join("install");
    fs::copy(&src_install, &dst_install).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}: {e}",
            src_install.display(),
            dst_install.display()
        )
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dst_install).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dst_install, perms).unwrap();
    }
    for template in [
        "daemon.yaml.production",
        "supervisor.yaml.production",
        // `--desktop` renders this one; every test copies it so a desktop run never fails on a
        // missing template rather than on what it is asserting.
        "desktop.yaml.production",
    ] {
        let prod = repo_root().join(template);
        fs::copy(&prod, dest.join(template)).unwrap_or_else(|e| {
            panic!(
                "copy {} -> {}/{template}: {e}",
                prod.display(),
                dest.display()
            )
        });
    }
    let dist = dest.join("packages").join("tddy-web").join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("index.html"), "<!DOCTYPE html><html></html>\n").unwrap();
}

fn write_fake_release_binaries(root: &Path) {
    let rel = root.join("target").join("release");
    fs::create_dir_all(&rel).unwrap();
    for name in [
        "tddy-supervisor",
        "tddy-daemon",
        "tddy-coder",
        "tddy-tools",
        "tddy-remote-git-repo",
        "tddy-session-sync",
        // Runs inside every jail the daemon spawns, so `install` requires it exactly as it
        // requires the daemon. A fixture missing it fails the preflight before any assertion runs.
        "tddy-sandbox-runner",
        // Spawned by the daemon as a sibling of its own executable, so `install` requires it for
        // the same reason and with the same preflight.
        "tddy-index-daemon",
    ] {
        let p = rel.join(name);
        fs::write(&p, b"fake-binary\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
        }
    }
}

/// Matches `install` script `resolve_codex_acp_native_src` / npm optional package names.
fn codex_acp_platform_pkg_dir() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("codex-acp-linux-x64"),
        ("linux", "aarch64") => Some("codex-acp-linux-arm64"),
        ("macos", "aarch64") => Some("codex-acp-darwin-arm64"),
        ("macos", "x86_64") => Some("codex-acp-darwin-x64"),
        _ => None,
    }
}

fn write_fake_codex_acp_native(root: &Path) {
    let Some(pkg) = codex_acp_platform_pkg_dir() else {
        return;
    };
    let p = root
        .join("node_modules")
        .join("@zed-industries")
        .join(pkg)
        .join("bin")
        .join("codex-acp");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, b"fake-codex-acp\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&p, perms).unwrap();
    }
}

fn run_install_in(root: &Path, env: &[(&str, &str)]) -> std::process::ExitStatus {
    run_install_args_in(root, &["--systemd"], env)
}

/// Run `install` with an explicit argument list (always the first arg is the script path).
fn run_install_args_in(
    root: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> std::process::ExitStatus {
    let mut cmd = Command::new("bash");
    cmd.current_dir(root);
    cmd.arg(root.join("install"));
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.status()
        .unwrap_or_else(|e| panic!("spawn install in {}: {e}", root.display()))
}

#[test]
fn install_copies_binaries_to_custom_dir() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let bin_dir = root.join("custom-bin");
    let cfg_dir = root.join("custom-etc");
    let sys_dir = root.join("custom-systemd");
    let web_dir = root.join("custom-web");

    // When
    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
            ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
        ],
    );

    // Then
    assert!(
        st.success(),
        "install should succeed with test env; got {st:?}"
    );
    for name in [
        "tddy-daemon",
        "tddy-coder",
        "tddy-tools",
        "tddy-remote-git-repo",
        "tddy-session-sync",
    ] {
        let p = bin_dir.join(name);
        assert!(p.is_file(), "expected {} installed", p.display());
        let body = fs::read_to_string(&p).unwrap();
        assert_eq!(body, "fake-binary\n");
    }
    if codex_acp_platform_pkg_dir().is_some() {
        let cap = bin_dir.join("codex-acp");
        assert!(cap.is_file(), "expected {} installed", cap.display());
        let body = fs::read_to_string(&cap).unwrap();
        assert_eq!(body, "fake-codex-acp\n");
    }
}

#[test]
fn install_creates_config_only_if_absent() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let bin_dir = root.join("b");
    let cfg_dir = root.join("c");
    let sys_dir = root.join("s");
    let web_dir = root.join("w");
    let env = [
        ("INSTALL_NO_SYSTEMCTL", "1"),
        ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
        ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
        ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
        ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
    ];

    // When — first install
    let st = run_install_in(root, &env);

    // Then — config is created
    assert!(st.success(), "first install: {st:?}");
    let cfg = cfg_dir.join("daemon.yaml");
    let first = fs::read_to_string(&cfg).unwrap();
    assert!(
        first.contains(bin_dir.to_str().unwrap()),
        "config must reference bin_dir"
    );

    // Given — pre-existing custom config
    fs::write(&cfg, "custom: preserved\n").unwrap();

    // When — second install
    let st2 = run_install_in(root, &env);

    // Then — custom config is preserved
    assert!(st2.success(), "second install: {st2:?}");
    let after = fs::read_to_string(&cfg).unwrap();
    assert_eq!(
        after, "custom: preserved\n",
        "config must not be overwritten"
    );
}

#[test]
fn install_generates_unit_with_correct_paths() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let bin_dir = root.join("mybin");
    let cfg_dir = root.join("mycfg");
    let sys_dir = root.join("mysystemd");
    let web_dir = root.join("myweb");

    // When
    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
            ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
        ],
    );

    // Then — the supervisor is the installed unit; it starts the daemon as a managed child.
    assert!(st.success(), "install: {st:?}");
    let unit = fs::read_to_string(sys_dir.join("tddy-supervisor.service")).unwrap();
    let cfg_file = cfg_dir.join("supervisor.yaml");
    let want_exec = format!(
        "ExecStart={}/tddy-supervisor -c {}",
        bin_dir.display(),
        cfg_file.display()
    );
    assert!(
        unit.contains(&want_exec),
        "unit file missing expected ExecStart line.\nGot:\n{unit}"
    );
}

#[test]
fn install_preserves_systemd_unit_unless_update_flag() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let bin_dir = root.join("bin");
    let cfg_dir = root.join("etc");
    let sys_dir = root.join("systemd");
    let web_dir = root.join("web");
    let base_env = [
        ("INSTALL_NO_SYSTEMCTL", "1"),
        ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
        ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
        ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
        ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
    ];

    // When — first install creates the unit
    let st = run_install_in(root, &base_env);

    // Then — unit is created; add a custom marker
    assert!(st.success(), "first install: {st:?}");
    let unit_path = sys_dir.join("tddy-supervisor.service");
    let mut unit = fs::read_to_string(&unit_path).unwrap();
    assert!(
        !unit.contains("User=preserve_test"),
        "template should not contain marker yet"
    );
    unit.push_str("\nUser=preserve_test\n");
    fs::write(&unit_path, &unit).unwrap();

    // When — second install without overwrite flag
    let st2 = run_install_in(root, &base_env);

    // Then — unit is preserved
    assert!(st2.success(), "second install: {st2:?}");
    let after = fs::read_to_string(&unit_path).unwrap();
    assert!(
        after.contains("User=preserve_test"),
        "unit must not be overwritten on reinstall; got:\n{after}"
    );

    // When — third install with --update-systemd-unit
    let st3 = run_install_args_in(root, &["--systemd", "--update-systemd-unit"], &base_env);

    // Then — unit is replaced
    assert!(st3.success(), "third install with overwrite: {st3:?}");
    let final_unit = fs::read_to_string(&unit_path).unwrap();
    assert!(
        !final_unit.contains("User=preserve_test"),
        "--update-systemd-unit should replace unit; got:\n{final_unit}"
    );
    let cfg_file = cfg_dir.join("supervisor.yaml");
    let want_exec = format!(
        "ExecStart={}/tddy-supervisor -c {}",
        bin_dir.display(),
        cfg_file.display()
    );
    assert!(
        final_unit.contains(&want_exec),
        "fresh unit should contain ExecStart; got:\n{final_unit}"
    );
}

#[test]
fn install_fails_without_binaries() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    let rel = root.join("target").join("release");
    fs::create_dir_all(&rel).unwrap();

    // When
    let st = run_install_in(root, &[("INSTALL_NO_SYSTEMCTL", "1")]);

    // Then
    assert!(
        !st.success(),
        "install should fail when release binaries are missing"
    );
}

#[test]
fn install_succeeds_without_codex_acp_native_when_not_required() {
    let Some(_) = codex_acp_platform_pkg_dir() else {
        return;
    };

    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    let bin_dir = root.join("bin");
    let cfg_dir = root.join("etc");
    let sys_dir = root.join("systemd");
    let web_dir = root.join("web");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&cfg_dir).unwrap();
    fs::create_dir_all(&sys_dir).unwrap();
    fs::create_dir_all(&web_dir).unwrap();

    // When
    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
            ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
        ],
    );

    // Then
    assert!(st.success(), "install should succeed when codex-acp is not required and node_modules native is absent; got {st:?}");
}

#[test]
fn install_fails_when_config_lists_codex_acp_without_native() {
    let Some(_) = codex_acp_platform_pkg_dir() else {
        return;
    };

    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    let cfg_dir = root.join("custom-etc");
    fs::create_dir_all(&cfg_dir).unwrap();
    fs::write(
        cfg_dir.join("daemon.yaml"),
        "allowed_agents:\n  - id: codex-acp\n    label: \"Codex ACP\"\n",
    )
    .unwrap();

    // When
    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
        ],
    );

    // Then
    assert!(
        !st.success(),
        "install should fail when allowed_agents lists codex-acp but native package is missing"
    );
}

#[test]
fn install_fails_when_install_bundle_codex_acp_without_native() {
    let Some(_) = codex_acp_platform_pkg_dir() else {
        return;
    };

    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);

    // When
    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BUNDLE_CODEX_ACP", "1"),
        ],
    );

    // Then
    assert!(
        !st.success(),
        "install should fail when INSTALL_BUNDLE_CODEX_ACP=1 but native package is missing"
    );
}

#[test]
fn install_user_flag_documented() {
    // Given
    let script = read_install();

    // When / Then
    verify_user_flag_support(&script);
}

#[test]
fn install_headless_flag_documented() {
    // Given
    let script = read_install();

    // When / Then
    verify_headless_flag_support(&script);
}

/// A `--user` install writes the unit into the invoking user's systemd dir, runs the daemon as that
/// user (no `User=`/`Group=`/`AppArmorProfile=`), targets `default.target`, and defaults every path
/// to XDG user locations under `$HOME` — all without root.
#[test]
fn install_user_mode_writes_user_service_and_config() {
    // Given — a fake $HOME so XDG defaults resolve under the tempdir (no INSTALL_* path overrides)
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let home_str = home.to_str().unwrap();

    // When
    let mut cmd = Command::new("bash");
    cmd.current_dir(root);
    cmd.arg(root.join("install"));
    cmd.args(["--systemd", "--user"]);
    cmd.env("INSTALL_NO_SYSTEMCTL", "1");
    cmd.env("HOME", home_str);
    // Clear ambient XDG so defaults are deterministic (~/.config, ~/.local/...).
    cmd.env_remove("XDG_CONFIG_HOME");
    cmd.env_remove("XDG_DATA_HOME");
    cmd.env_remove("XDG_STATE_HOME");
    let st = cmd
        .status()
        .unwrap_or_else(|e| panic!("spawn install: {e}"));

    // Then — unit lands in the user systemd dir and is a user-manager unit
    assert!(st.success(), "user install: {st:?}");
    let unit_path = home.join(".config/systemd/user/tddy-daemon.service");
    let unit = fs::read_to_string(&unit_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", unit_path.display()));
    assert!(
        unit.contains("WantedBy=default.target"),
        "user unit must target default.target; got:\n{unit}"
    );
    assert!(
        !unit.contains("User=") && !unit.contains("Group="),
        "user unit must not set User=/Group= (runs as invoking user); got:\n{unit}"
    );
    assert!(
        !unit.contains("AppArmorProfile="),
        "user unit must not reference an AppArmor profile; got:\n{unit}"
    );
    let want_exec = format!(
        "ExecStart={}/tddy-daemon -c {}",
        home.join(".local/bin").display(),
        home.join(".config/tddy/daemon.yaml").display()
    );
    assert!(
        unit.contains(&want_exec),
        "user unit ExecStart should point at XDG user paths; want {want_exec}\ngot:\n{unit}"
    );

    // Config is created under XDG config with user-writable log + auth paths
    let cfg = fs::read_to_string(home.join(".config/tddy/daemon.yaml")).unwrap();
    assert!(
        cfg.contains(home.join(".local/state/tddy-daemon").to_str().unwrap()),
        "user config log path must be user-writable; got:\n{cfg}"
    );
    assert!(
        cfg.contains(home.join(".local/share/tddy/auth").to_str().unwrap()),
        "user config auth_storage must be user-writable; got:\n{cfg}"
    );
}

/// A `--user` install is the developer-machine mode, so the `GIT_SSH_COMMAND` shim has to land in
/// the user's own bin directory — `git` resolves it on `PATH`, and nothing else installs it.
#[test]
fn install_user_mode_puts_the_git_ssh_shim_on_the_user_bin_path() {
    // Given — a fake $HOME so XDG defaults resolve under the tempdir (no INSTALL_* path overrides)
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();

    // When
    let mut cmd = Command::new("bash");
    cmd.current_dir(root);
    cmd.arg(root.join("install"));
    cmd.args(["--systemd", "--user"]);
    cmd.env("INSTALL_NO_SYSTEMCTL", "1");
    cmd.env("HOME", home.to_str().unwrap());
    cmd.env_remove("XDG_CONFIG_HOME");
    cmd.env_remove("XDG_DATA_HOME");
    cmd.env_remove("XDG_STATE_HOME");
    let st = cmd
        .status()
        .unwrap_or_else(|e| panic!("spawn install: {e}"));

    // Then
    assert!(st.success(), "user install: {st:?}");
    let shim = home.join(".local/bin/tddy-remote-git-repo");
    assert!(
        shim.is_file(),
        "expected the git shim at {}",
        shim.display()
    );
    assert_eq!(fs::read_to_string(&shim).unwrap(), "fake-binary\n");

    // The worktree mirror is a client too, and the script installs it for the same reason: a
    // binary whose own documentation says "put it on PATH" that no script ever puts anywhere is a
    // binary nobody has.
    let sync = home.join(".local/bin/tddy-session-sync");
    assert!(
        sync.is_file(),
        "expected the worktree mirror at {}",
        sync.display()
    );
    assert_eq!(fs::read_to_string(&sync).unwrap(), "fake-binary\n");
}

/// `--headless` installs without the built tddy-web bundle: the `dist` check is skipped, nothing is
/// copied into the web bundle dir, and the daemon config still points `web_bundle_path` at it.
#[test]
fn install_headless_skips_web_bundle() {
    // Given — no tddy-web dist at all
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    fs::remove_dir_all(root.join("packages/tddy-web/dist")).unwrap();
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);
    let bin_dir = root.join("b");
    let cfg_dir = root.join("c");
    let sys_dir = root.join("s");
    let web_dir = root.join("w");

    // When
    let st = run_install_args_in(
        root,
        &["--systemd", "--headless"],
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
            ("INSTALL_WEB_BUNDLE_DIR", web_dir.to_str().unwrap()),
        ],
    );

    // Then — install succeeds despite no dist, and the web bundle dir is empty
    assert!(
        st.success(),
        "headless install should succeed without tddy-web dist; got {st:?}"
    );
    let entries = fs::read_dir(&web_dir).map(|d| d.count()).unwrap_or(0);
    assert_eq!(entries, 0, "headless install must not copy any web assets");
    let cfg = fs::read_to_string(cfg_dir.join("daemon.yaml")).unwrap();
    assert!(
        cfg.contains(web_dir.to_str().unwrap()),
        "config should still declare web_bundle_path (empty dir); got:\n{cfg}"
    );
}

/// Without `--headless`, a missing tddy-web bundle is a hard error (guards against silently shipping
/// a UI-less daemon).
#[test]
fn install_without_headless_requires_web_bundle() {
    // Given — no tddy-web dist
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    fs::remove_dir_all(root.join("packages/tddy-web/dist")).unwrap();
    write_fake_release_binaries(root);

    // When
    let st = run_install_in(root, &[("INSTALL_NO_SYSTEMCTL", "1")]);

    // Then
    assert!(
        !st.success(),
        "install should fail without --headless when the tddy-web bundle is missing"
    );
}

// --- `install --desktop`: the Tauri application, not a systemd-managed daemon ---------------------

/// The artifacts `tauri build` leaves behind, on whichever platform the test is running: a `.app`
/// bundle on macOS, a plain binary on Linux. `install --desktop` preflights for exactly one of
/// these and refuses to install without it.
fn write_fake_desktop_artifacts(root: &Path) {
    let rel = root.join("target").join("release");
    if cfg!(target_os = "macos") {
        let macos = rel
            .join("bundle")
            .join("macos")
            .join("Tddy Desktop.app")
            .join("Contents")
            .join("MacOS");
        fs::create_dir_all(&macos).unwrap();
        write_executable(&macos.join("tddy-desktop"), "fake-app\n");
        fs::write(
            macos.parent().unwrap().join("Info.plist"),
            "<plist></plist>\n",
        )
        .unwrap();
    } else {
        write_executable(&rel.join("tddy-desktop"), "fake-app\n");
    }
    let icons = root
        .join("packages")
        .join("tddy-desktop")
        .join("src-tauri")
        .join("icons");
    fs::create_dir_all(&icons).unwrap();
    fs::write(icons.join("128x128.png"), b"\x89PNG\r\n").unwrap();
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// A tree with everything `--desktop` needs, and the env that redirects every path it writes into
/// that tree. `HOME` is redirected too: the install derives `~/.tddy` from it, and a test that let
/// the real one through would write into the developer's own install.
struct DesktopFixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin_dir: PathBuf,
    tddy_home: PathBuf,
    app_dir: PathBuf,
    xdg_data_dir: PathBuf,
}

impl DesktopFixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        copy_install_tree(&root);
        write_fake_release_binaries(&root);
        write_fake_desktop_artifacts(&root);
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        Self {
            bin_dir: root.join("desktop-bin"),
            // The installed application reads $HOME/.tddy/desktop.yaml and nothing else, so the
            // fixture keeps TDDY_HOME where the application would look under the redirected HOME.
            tddy_home: home.join(".tddy"),
            app_dir: root.join("Applications"),
            xdg_data_dir: root.join("xdg-data"),
            home,
            root,
            _tmp: tmp,
        }
    }

    fn run(&self) -> std::process::ExitStatus {
        run_install_args_in(
            &self.root,
            &["--desktop"],
            &[
                ("HOME", self.home.to_str().unwrap()),
                ("INSTALL_BIN_DIR", self.bin_dir.to_str().unwrap()),
                ("INSTALL_TDDY_HOME", self.tddy_home.to_str().unwrap()),
                ("INSTALL_DESKTOP_APP_DIR", self.app_dir.to_str().unwrap()),
                ("INSTALL_XDG_DATA_DIR", self.xdg_data_dir.to_str().unwrap()),
            ],
        )
    }

    fn config(&self) -> PathBuf {
        self.tddy_home.join("desktop.yaml")
    }
}

#[test]
fn install_desktop_flag_documented() {
    // Given
    let script = read_install();

    // When / Then
    verify_desktop_flag_support(&script);
}

#[test]
fn desktop_template_names_the_signin_callback_port_and_no_stray_placeholder() {
    // Given
    let script = read_install();
    let template = fs::read_to_string(repo_root().join("desktop.yaml.production"))
        .expect("read desktop.yaml.production");

    // When / Then
    verify_desktop_config_template(&script, &template);
}

#[test]
fn install_desktop_puts_the_application_and_the_tools_it_spawns_on_path() {
    // Given
    let fx = DesktopFixture::new();

    // When
    let st = fx.run();

    // Then — the application resolves by name…
    assert!(st.success(), "install --desktop: {st:?}");
    let launcher = fx.bin_dir.join("tddy-desktop");
    assert!(
        launcher.is_file(),
        "expected the launcher at {}",
        launcher.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&launcher).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "the launcher must be executable");
    }

    // …and so do the binaries the embedded daemon spawns and desktop.yaml names by absolute path.
    for name in [
        "tddy-coder",
        "tddy-tools",
        "tddy-sandbox-runner",
        "tddy-index-daemon",
        "tddy-remote-git-repo",
        "tddy-session-sync",
    ] {
        assert!(
            fx.bin_dir.join(name).is_file(),
            "expected {name} on PATH at {}",
            fx.bin_dir.display()
        );
    }

    // The application's own process IS the daemon: a second daemon binary would be a service
    // nothing starts, contending for the same data directory.
    for absent in ["tddy-daemon", "tddy-supervisor"] {
        assert!(
            !fx.bin_dir.join(absent).exists(),
            "--desktop must not install {absent}"
        );
    }
}

#[test]
fn install_desktop_renders_the_one_config_a_release_build_reads() {
    // Given
    let fx = DesktopFixture::new();

    // When
    let st = fx.run();

    // Then — the config exists where the application looks, fully substituted.
    assert!(st.success(), "install --desktop: {st:?}");
    let config = fs::read_to_string(fx.config())
        .unwrap_or_else(|e| panic!("read {}: {e}", fx.config().display()));
    assert!(
        !config.contains("__"),
        "every placeholder must be substituted; got:\n{config}"
    );
    assert!(
        config.contains(&format!("tddy_data_dir: \"{}\"", fx.tddy_home.display())),
        "config must name the installed data dir; got:\n{config}"
    );
    assert!(
        config.contains(&format!("{}/tddy-coder", fx.bin_dir.display())),
        "allowed_tools must point at the binaries this run installed; got:\n{config}"
    );

    // Retained GitHub tokens land here, so the umask must not get a say.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let auth = fx.tddy_home.join("auth");
        let mode = fs::metadata(&auth)
            .unwrap_or_else(|e| panic!("stat {}: {e}", auth.display()))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "auth storage must be 0700");
    }
}

#[test]
fn install_desktop_never_overwrites_an_operator_edited_config() {
    // Given — an install, then an operator edit
    let fx = DesktopFixture::new();
    assert!(fx.run().success(), "first install");
    let edited = format!(
        "{}\n# operator edit\n",
        fs::read_to_string(fx.config()).unwrap()
    );
    fs::write(fx.config(), &edited).unwrap();

    // When — a reinstall
    let st = fx.run();

    // Then — the edit survives
    assert!(st.success(), "reinstall: {st:?}");
    assert_eq!(
        fs::read_to_string(fx.config()).unwrap(),
        edited,
        "a reinstall must not overwrite the config"
    );
}

#[test]
fn install_desktop_refuses_when_the_application_has_not_been_built() {
    // Given — a tree with the CLI binaries but no built application
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();

    // When
    let st = run_install_args_in(
        root,
        &["--desktop"],
        &[
            ("HOME", home.to_str().unwrap()),
            ("INSTALL_BIN_DIR", root.join("b").to_str().unwrap()),
            ("INSTALL_TDDY_HOME", home.join(".tddy").to_str().unwrap()),
        ],
    );

    // Then — nothing is installed, rather than a launcher that runs a binary that is not there.
    assert!(
        !st.success(),
        "--desktop must refuse without a built application"
    );
    assert!(
        !root.join("b").join("tddy-desktop").exists(),
        "a refused install must leave no launcher behind"
    );
}

#[test]
fn install_refuses_the_two_deployments_in_one_run() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);

    // When
    let st = run_install_args_in(
        root,
        &["--systemd", "--desktop"],
        &[("INSTALL_NO_SYSTEMCTL", "1")],
    );

    // Then — two daemons on one machine would contend for the same data dir and LiveKit identity.
    assert!(
        !st.success(),
        "--systemd and --desktop together must be an error"
    );
}
