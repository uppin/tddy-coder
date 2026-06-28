//! Acceptance: darwin Seatbelt write/read confinement for sandboxed processes.
//!
//! Requires macOS `sandbox-exec`. Skipped on other platforms.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use tddy_sandbox::format_egress_logs;
use tddy_sandbox::SandboxSpec;

fn assert_sandbox_exit(egress: &Path, exit: i32, expect_success: bool, context: &str) {
    assert_ne!(
        exit,
        6,
        "{context}: sandbox-exec profile invalid (exit 6)\n{}",
        format_egress_logs(egress)
    );
    if expect_success {
        assert_eq!(
            exit,
            0,
            "{context}: expected exit 0, got {exit}\n{}",
            format_egress_logs(egress)
        );
    } else {
        assert_ne!(
            exit,
            0,
            "{context}: expected non-zero exit, got 0\n{}",
            format_egress_logs(egress)
        );
    }
}

fn run_in_sandbox_with_command(project_root: &Path, egress: &Path, command: Vec<String>) -> i32 {
    let scratch = project_root.join(".work");
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::create_dir_all(scratch.join("tmp")).unwrap();
    std::fs::create_dir_all(egress).unwrap();

    let profile_path = project_root.join("profile.sb");
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "HOME".into(),
        scratch.join("home").to_string_lossy().to_string(),
    );
    env.insert(
        "TMPDIR".into(),
        scratch.join("tmp").to_string_lossy().to_string(),
    );
    env.insert("PATH".into(), "/usr/bin:/bin".into());

    let spec = SandboxSpec {
        project_root: project_root.to_path_buf(),
        scratch_dir: scratch,
        egress_dir: egress.to_path_buf(),
        allow_read_paths: tddy_sandbox_darwin::detect_allow_read_paths(),
        command,
        env,
        profile_path,
        loopback_allow_ports: vec![],
        ipc_socket: None,
    };

    let mut handle = tddy_sandbox_darwin::spawn(spec).expect("sandbox spawn must succeed");
    let status = handle.child_mut().wait().expect("wait for sandbox child");
    status.code().unwrap_or(1)
}

fn run_in_sandbox_expect_failure(
    project_root: &Path,
    egress: &Path,
    shell_script: &str,
    context: &str,
) {
    let exit = run_in_sandbox(project_root, egress, shell_script);
    assert_sandbox_exit(egress, exit, false, context);
}

fn run_in_sandbox(project_root: &Path, egress: &Path, shell_script: &str) -> i32 {
    run_in_sandbox_with_command(
        project_root,
        egress,
        vec!["/bin/sh".into(), "-c".into(), shell_script.to_string()],
    )
}

/// **seatbelt_denies_writes_outside_project_tree**: a confined process cannot create files
/// in the real home directory.
#[test]
fn seatbelt_denies_writes_outside_project_tree() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(&project_root).unwrap();
    let home = std::env::var("HOME").expect("HOME must be set for confinement test");
    let escape_probe = PathBuf::from(&home).join(".sandbox-escape-probe");
    let _ = std::fs::remove_file(&escape_probe);

    // When — escape write must fail (non-zero exit)
    run_in_sandbox_expect_failure(
        &project_root,
        &egress,
        &format!("touch '{}'", escape_probe.display()),
        "seatbelt_denies_writes_outside_project_tree",
    );

    // Then
    assert!(
        !escape_probe.exists(),
        "escape probe must not exist at {}",
        escape_probe.display()
    );
    assert!(
        egress.join(tddy_sandbox::SANDBOX_SPAWN_MANIFEST).exists(),
        "spawn manifest must be written to egress for diagnostics"
    );
    assert!(
        egress.join(tddy_sandbox::SANDBOX_EXEC_STDERR_LOG).exists(),
        "sandbox-exec stderr log must be captured in egress"
    );
}

/// **seatbelt_allows_broad_reads_but_still_denies_writes**: the SBPL profile intentionally grants
/// blanket `(allow file-read*)` — the V8/Node `claude` binary reads OS caches, timezone data, dyld
/// state, and user config from many paths outside any explicit allow-list, and SIGTRAPs at startup
/// without it (confirmed: `(allow file-read*)` is the rule that lets `claude --version` run).
///
/// Read confinement is therefore deliberately traded away for the Node agent. The security boundary
/// is **write** confinement (covered by `seatbelt_denies_writes_outside_project_tree`): this test
/// pins the trade-off by asserting a read outside the project tree SUCCEEDS while a write to the
/// same location FAILS.
#[test]
fn seatbelt_allows_broad_reads_but_still_denies_writes() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(&project_root).unwrap();
    let home = std::env::var("HOME").expect("HOME");
    let probe = PathBuf::from(&home).join(".tddy-sandbox-read-probe.txt");
    std::fs::write(&probe, "top-secret").unwrap();

    // When/Then — read outside the project tree is allowed (blanket file-read*)…
    let read_exit = run_in_sandbox(
        &project_root,
        &egress,
        &format!("cat '{}'", probe.display()),
    );
    assert_sandbox_exit(
        &egress,
        read_exit,
        true,
        "seatbelt_allows_broad_reads_but_still_denies_writes (read)",
    );

    // …but writing to that same out-of-tree path is still denied.
    run_in_sandbox_expect_failure(
        &project_root,
        &egress,
        &format!("touch '{}.written'", probe.display()),
        "seatbelt_allows_broad_reads_but_still_denies_writes (write)",
    );
    assert!(
        !PathBuf::from(format!("{}.written", probe.display())).exists(),
        "out-of-tree write must not succeed"
    );

    let _ = std::fs::remove_file(&probe);
}
