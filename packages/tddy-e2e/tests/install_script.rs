//! Contract and functional tests for repo-root `install` (systemd install flow).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_e2e::install_contract::{
    verify_build_flag_invokes_release, verify_env_override_references,
    verify_install_script_contracts, verify_no_systemctl_support, verify_requires_systemd_flag,
    verify_root_check, verify_syntax,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn install_path() -> PathBuf {
    repo_root().join("install")
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
fn install_no_systemctl_support() {
    verify_no_systemctl_support(&read_install());
}

#[test]
fn install_build_flag_accepted() {
    verify_build_flag_invokes_release(&read_install());
}

#[test]
fn install_full_contract_orchestration() {
    verify_install_script_contracts(&install_path());
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

    let bin_dir = root.join("custom-bin");
    let cfg_dir = root.join("custom-etc");
    let sys_dir = root.join("custom-systemd");

    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
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
}

#[test]
fn install_creates_config_only_if_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);

    let bin_dir = root.join("b");
    let cfg_dir = root.join("c");
    let sys_dir = root.join("s");

    let env = [
        ("INSTALL_NO_SYSTEMCTL", "1"),
        ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
        ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
        ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
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

    let bin_dir = root.join("mybin");
    let cfg_dir = root.join("mycfg");
    let sys_dir = root.join("mysystemd");

    let st = run_install_in(
        root,
        &[
            ("INSTALL_NO_SYSTEMCTL", "1"),
            ("INSTALL_BIN_DIR", bin_dir.to_str().unwrap()),
            ("INSTALL_CONFIG_DIR", cfg_dir.to_str().unwrap()),
            ("INSTALL_SYSTEMD_DIR", sys_dir.to_str().unwrap()),
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
