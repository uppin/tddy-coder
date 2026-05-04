//! Contract and functional tests for repo-root `install` (systemd install flow).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_e2e::install_contract::{
    verify_build_flag_invokes_release, verify_env_override_references,
    verify_install_overwrite_systemd_unit, verify_install_script_contracts,
    verify_no_systemctl_support, verify_requires_systemd_flag, verify_root_check, verify_syntax,
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
    verify_syntax(&install_path());
}

#[test]
fn install_requires_systemd_flag() {
    verify_requires_systemd_flag(&read_install());
}

#[test]
fn install_respects_env_overrides() {
    verify_env_override_references(&read_install());
}

#[test]
fn install_has_root_check() {
    verify_root_check(&read_install());
}

#[test]
fn install_overwrite_systemd_unit_documented() {
    verify_install_overwrite_systemd_unit(&read_install());
}

#[test]
fn install_no_systemctl_support() {
    verify_no_systemctl_support(&read_install());
}

#[test]
fn install_build_flag_accepted() {
    verify_build_flag_invokes_release(&read_install());
}

#[test]
fn install_full_contract_orchestration() {
    verify_install_script_contracts(&install_path(), &daemon_yaml_production_path());
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
    let prod = repo_root().join("daemon.yaml.production");
    fs::copy(&prod, dest.join("daemon.yaml.production")).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}/daemon.yaml.production: {e}",
            prod.display(),
            dest.display()
        )
    });
    let dist = dest.join("packages").join("tddy-web").join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("index.html"), "<!DOCTYPE html><html></html>\n").unwrap();
}

fn write_fake_release_binaries(root: &Path) {
    let rel = root.join("target").join("release");
    fs::create_dir_all(&rel).unwrap();
    for name in ["tddy-daemon", "tddy-coder", "tddy-tools"] {
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
    let mut cmd = Command::new("bash");
    cmd.current_dir(root);
    cmd.arg(root.join("install"));
    cmd.args(["--systemd"]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.status()
        .unwrap_or_else(|e| panic!("spawn install in {}: {e}", root.display()))
}

#[test]
fn install_copies_binaries_to_custom_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);

    let bin_dir = root.join("custom-bin");
    let cfg_dir = root.join("custom-etc");
    let sys_dir = root.join("custom-systemd");
    let web_dir = root.join("custom-web");

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
    assert!(
        st.success(),
        "install should succeed with test env; got {st:?}"
    );

    for name in ["tddy-daemon", "tddy-coder", "tddy-tools"] {
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

    let st = run_install_in(root, &env);
    assert!(st.success(), "first install: {st:?}");
    let cfg = cfg_dir.join("daemon.yaml");
    let first = fs::read_to_string(&cfg).unwrap();
    assert!(first.contains(bin_dir.to_str().unwrap()));

    fs::write(&cfg, "custom: preserved\n").unwrap();

    let st2 = run_install_in(root, &env);
    assert!(st2.success(), "second install: {st2:?}");
    let after = fs::read_to_string(&cfg).unwrap();
    assert_eq!(
        after, "custom: preserved\n",
        "config must not be overwritten"
    );
}

#[test]
fn install_generates_unit_with_correct_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    write_fake_codex_acp_native(root);

    let bin_dir = root.join("mybin");
    let cfg_dir = root.join("mycfg");
    let sys_dir = root.join("mysystemd");
    let web_dir = root.join("myweb");

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
    assert!(st.success(), "install: {st:?}");

    let unit = fs::read_to_string(sys_dir.join("tddy-daemon.service")).unwrap();
    let cfg_file = cfg_dir.join("daemon.yaml");
    let want_exec = format!(
        "ExecStart={}/tddy-daemon -c {}",
        bin_dir.display(),
        cfg_file.display()
    );
    assert!(
        unit.contains(&want_exec),
        "unit file missing expected ExecStart line.\nGot:\n{unit}"
    );
}

#[test]
fn install_preserves_systemd_unit_unless_overwrite_env() {
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

    let st = run_install_in(root, &base_env);
    assert!(st.success(), "first install: {st:?}");

    let unit_path = sys_dir.join("tddy-daemon.service");
    let mut unit = fs::read_to_string(&unit_path).unwrap();
    assert!(
        !unit.contains("User=preserve_test"),
        "template should not contain marker yet"
    );
    unit.push_str("\nUser=preserve_test\n");
    fs::write(&unit_path, &unit).unwrap();

    let st2 = run_install_in(root, &base_env);
    assert!(st2.success(), "second install: {st2:?}");
    let after = fs::read_to_string(&unit_path).unwrap();
    assert!(
        after.contains("User=preserve_test"),
        "unit must not be overwritten on reinstall; got:\n{after}"
    );

    let mut env_overwrite: Vec<(&str, &str)> = base_env.to_vec();
    env_overwrite.push(("INSTALL_OVERWRITE_SYSTEMD_UNIT", "1"));
    let st3 = run_install_in(root, &env_overwrite);
    assert!(st3.success(), "third install with overwrite: {st3:?}");
    let final_unit = fs::read_to_string(&unit_path).unwrap();
    assert!(
        !final_unit.contains("User=preserve_test"),
        "INSTALL_OVERWRITE_SYSTEMD_UNIT=1 should replace unit; got:\n{final_unit}"
    );
    let cfg_file = cfg_dir.join("daemon.yaml");
    let want_exec = format!(
        "ExecStart={}/tddy-daemon -c {}",
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
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    let rel = root.join("target").join("release");
    fs::create_dir_all(&rel).unwrap();

    let st = run_install_in(root, &[("INSTALL_NO_SYSTEMCTL", "1")]);
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
    assert!(
        st.success(),
        "install should succeed when codex-acp is not required and node_modules native is absent; got {st:?}"
    );
}

#[test]
fn install_fails_when_config_lists_codex_acp_without_native() {
    let Some(_) = codex_acp_platform_pkg_dir() else {
        return;
    };
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

    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
        ],
    );
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
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);

    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BUNDLE_CODEX_ACP", "1"),
        ],
    );
    assert!(
        !st.success(),
        "install should fail when INSTALL_BUNDLE_CODEX_ACP=1 but native package is missing"
    );
}
