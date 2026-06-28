//! Acceptance: darwin Seatbelt write/read confinement for sandboxed processes.
//!
//! Requires macOS `sandbox-exec`. Skipped on other platforms.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_sandbox::format_egress_logs;
use tddy_sandbox::{
    claude_policy, claude_required_copies, claude_required_reads, NetworkSpec, SandboxBuilder,
    SandboxPlan,
};

/// Locate the real `claude` binary, canonicalized. Prefers the newest versioned binary under
/// `~/.local/share/claude/versions` because the `claude` on PATH may be a wrapper script that
/// itself re-searches PATH (which the jail deliberately trims). Tests that need it skip (return
/// early) when it is absent — a precondition of the host, not a branch on the result under test.
fn which_claude() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let versions = PathBuf::from(&home).join(".local/share/claude/versions");
        if let Ok(entries) = std::fs::read_dir(&versions) {
            let mut bins: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            bins.sort();
            if let Some(latest) = bins.pop() {
                return Some(latest);
            }
        }
    }
    let out = std::process::Command::new("which")
        .arg("claude")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(std::fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(&path)))
}

/// Build a strict (no-wildcard) plan for a Claude jail: the explicit Claude read recipe + policy,
/// `.credentials.json` seeded into the scratch HOME, OAuth loopback inbound allowed.
fn strict_claude_plan(
    project_root: &Path,
    egress: &Path,
    command: Vec<String>,
    claude_bin: &Path,
) -> SandboxPlan {
    let scratch = project_root.join(".work");
    let scratch_home = scratch.join("home");
    std::fs::create_dir_all(&scratch_home).unwrap();
    std::fs::create_dir_all(scratch.join("tmp")).unwrap();
    std::fs::create_dir_all(egress).unwrap();
    let host_home = PathBuf::from(std::env::var("HOME").expect("HOME must be set"));

    let mut env = BTreeMap::new();
    env.insert("HOME".into(), scratch_home.to_string_lossy().to_string());
    env.insert(
        "TMPDIR".into(),
        scratch.join("tmp").to_string_lossy().to_string(),
    );
    env.insert("PATH".into(), "/usr/bin:/bin".into());

    SandboxBuilder::new(project_root, scratch, egress, command)
        .profile_path(project_root.join("profile.sb"))
        .reads(claude_required_reads(claude_bin))
        .copies(claude_required_copies(&host_home, &scratch_home))
        .policy(claude_policy())
        .network(NetworkSpec {
            loopback_allow_ports: vec![],
            allow_oauth_inbound: true,
        })
        .env_map(env)
        .build()
        .expect("strict plan must build")
}

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

/// Build a strict plan for a plain shell command (no Claude binary needed): the OS baseline reads +
/// policy are enough to boot `/bin/sh`, while writes stay confined to the project tree.
fn strict_system_plan(project_root: &Path, egress: &Path, command: Vec<String>) -> SandboxPlan {
    let scratch = project_root.join(".work");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::create_dir_all(scratch.join("tmp")).unwrap();
    std::fs::create_dir_all(egress).unwrap();

    let mut env = BTreeMap::new();
    env.insert(
        "HOME".into(),
        scratch.join("home").to_string_lossy().to_string(),
    );
    env.insert(
        "TMPDIR".into(),
        scratch.join("tmp").to_string_lossy().to_string(),
    );
    env.insert("PATH".into(), "/usr/bin:/bin".into());

    SandboxBuilder::new(project_root, scratch, egress, command)
        .profile_path(project_root.join("profile.sb"))
        .reads(tddy_sandbox::system_baseline_reads())
        .policy(tddy_sandbox::claude_policy())
        .network(NetworkSpec::default())
        .env_map(env)
        .build()
        .expect("strict system plan must build")
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
    let plan = strict_system_plan(
        &project_root,
        &egress,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("touch '{}'", escape_probe.display()),
        ],
    );

    // When — escape write must fail (non-zero exit)
    let mut handle = tddy_sandbox_darwin::spawn_plan(plan).expect("sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for sandbox child")
        .code()
        .unwrap_or(1);
    assert_sandbox_exit(
        &egress,
        exit,
        false,
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

/// **a_strict_profile_still_lets_the_claude_binary_report_its_version**: the strict-reads gate. A
/// plan built from the explicit Claude read recipe — with NO blanket `(allow file-read*)` — must
/// still let the V8/Node `claude` binary boot far enough to print its version (exit 0). This is the
/// proof that the explicit read allow-list is complete enough to replace the wildcard.
#[test]
fn a_strict_profile_still_lets_the_claude_binary_report_its_version() {
    // Given
    let Some(claude_bin) = which_claude() else {
        eprintln!("skip: claude not found on PATH");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(&project_root).unwrap();
    let plan = strict_claude_plan(
        &project_root,
        &egress,
        vec![
            claude_bin.to_string_lossy().into_owned(),
            "--version".into(),
        ],
        &claude_bin,
    );

    // When
    let mut handle =
        tddy_sandbox_darwin::spawn_plan(plan).expect("strict sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for child")
        .code()
        .unwrap_or(1);

    // Then
    assert_sandbox_exit(
        &egress,
        exit,
        true,
        "a_strict_profile_still_lets_the_claude_binary_report_its_version",
    );
}

/// **a_strict_profile_denies_reading_a_path_not_on_the_allow_list**: with the wildcard gone, a read
/// of an out-of-tree path the plan never declared is denied (the command exits non-zero). This pins
/// the read-confinement boundary the wildcard removal restores.
#[test]
fn a_strict_profile_denies_reading_a_path_not_on_the_allow_list() {
    // Given
    let Some(claude_bin) = which_claude() else {
        eprintln!("skip: claude not found on PATH");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(&project_root).unwrap();
    let home = std::env::var("HOME").expect("HOME");
    let probe = PathBuf::from(&home).join(".tddy-strict-read-probe.txt");
    std::fs::write(&probe, "top-secret").unwrap();
    let plan = strict_claude_plan(
        &project_root,
        &egress,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("cat '{}'", probe.display()),
        ],
        &claude_bin,
    );

    // When — reading the undeclared out-of-tree path must fail under strict reads
    let mut handle =
        tddy_sandbox_darwin::spawn_plan(plan).expect("strict sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for child")
        .code()
        .unwrap_or(1);

    // Then
    assert_sandbox_exit(
        &egress,
        exit,
        false,
        "a_strict_profile_denies_reading_a_path_not_on_the_allow_list",
    );

    let _ = std::fs::remove_file(&probe);
}
