//! Acceptance: darwin Seatbelt write/read confinement for sandboxed processes.
//!
//! Requires macOS `sandbox-exec`. Skipped on other platforms.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_sandbox::format_egress_logs;
use tddy_sandbox::{NetworkSpec, SandboxBuilder, SandboxPlan};
use tddy_sandbox_recipes::{
    claude_credentials_copies, claude_interactive_policy, process_claude_exec_reads,
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
        .reads(process_claude_exec_reads(claude_bin))
        .copies(claude_credentials_copies(&host_home, &scratch_home))
        .policy(claude_interactive_policy())
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
        .policy(tddy_sandbox_recipes::shell_interactive_policy())
        .network(NetworkSpec::default())
        .env_map(env)
        .build()
        .expect("strict system plan must build")
}

/// **a_strict_profile_lets_a_shell_read_dev_null**: shells and tools open `/dev/null` (and the
/// other standard device nodes) `O_RDWR`, so the strict read allow-list must grant them read — not
/// just the write/ioctl allows. Without it, shell startup inside the jail fails with
/// "operation not permitted: /dev/null".
#[test]
fn a_strict_profile_lets_a_shell_read_dev_null() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(&project_root).unwrap();
    let plan = strict_system_plan(
        &project_root,
        &egress,
        vec!["/bin/sh".into(), "-c".into(), "cat /dev/null".into()],
    );

    // When
    let mut handle = tddy_sandbox_darwin::spawn_plan(plan).expect("sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for sandbox child")
        .code()
        .unwrap_or(1);

    // Then
    assert_sandbox_exit(
        &egress,
        exit,
        true,
        "a_strict_profile_lets_a_shell_read_dev_null",
    );
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

// ─── agent context dir: writable from the host, read-only inside the jail ───────────────────

/// The guidance file the managed-codebase preamble lives in. Overwriting it is the concrete attack
/// the context-dir carve-out exists to stop: an agent that rewrites its own `CLAUDE.md` erases the
/// one instruction telling it the real codebase is somewhere else.
const AGENT_GUIDANCE_FILE: &str = "CLAUDE.md";
const AGENT_GUIDANCE_TEXT: &str = "# Managed codebase\nThe codebase is not in this directory.\n";

/// A project tree on `/private/tmp` rather than under `TMPDIR`. The profile grants the whole
/// `/var/folders` tree (the OS per-user scratch base) write access, so a project root inside it
/// would be writable no matter what the project-root rule said — and these tests must observe the
/// context-dir deny beating the *project-root* allow specifically.
fn a_project_tree_outside_the_os_scratch_base(name: &str) -> PathBuf {
    let root = PathBuf::from("/private/tmp").join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("project")).expect("create project root");
    root
}

/// Seed the agent's context dir with its guidance file, the way the host-side context syncer does
/// before the jail starts.
fn a_context_dir_holding_agent_guidance(project_root: &Path) -> PathBuf {
    let context_dir = project_root.join("context");
    std::fs::create_dir_all(&context_dir).expect("create context dir");
    std::fs::write(context_dir.join(AGENT_GUIDANCE_FILE), AGENT_GUIDANCE_TEXT)
        .expect("seed agent guidance");
    context_dir
}

/// Like [`strict_system_plan`], but the confined command also carries `--context-dir <path>` — the
/// flag every real `tddy-sandbox-runner` argv uses to name the agent's context dir, and the one the
/// profile renderer derives its carve-out from. `/bin/sh -c <script>` assigns trailing operands to
/// `$0`/`$1` and still runs the script unchanged, so the flag rides along without altering the probe.
fn strict_system_plan_declaring_a_context_dir(
    project_root: &Path,
    egress: &Path,
    script: String,
    context_dir: &Path,
) -> SandboxPlan {
    strict_system_plan(
        project_root,
        egress,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            script,
            "--context-dir".into(),
            context_dir.to_string_lossy().into_owned(),
        ],
    )
}

/// **seatbelt_denies_a_confined_process_overwriting_the_guidance_in_its_context_dir**: the context
/// dir is inside the project root's writable subpath, so only the explicit carve-out stops the
/// agent from rewriting its own instructions. The file stays byte-for-byte as the host wrote it.
#[test]
fn seatbelt_denies_a_confined_process_overwriting_the_guidance_in_its_context_dir() {
    // Given
    let tree = a_project_tree_outside_the_os_scratch_base("tddy-context-deny-overwrite");
    let project_root = tree.join("project");
    let egress = tree.join("egress");
    let context_dir = a_context_dir_holding_agent_guidance(&project_root);
    let guidance = context_dir.join(AGENT_GUIDANCE_FILE);
    let plan = strict_system_plan_declaring_a_context_dir(
        &project_root,
        &egress,
        format!("echo overwritten > '{}'", guidance.display()),
        &context_dir,
    );

    // When
    let mut handle = tddy_sandbox_darwin::spawn_plan(plan).expect("sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for sandbox child")
        .code()
        .unwrap_or(1);

    // Then
    assert_sandbox_exit(
        &egress,
        exit,
        false,
        "seatbelt_denies_a_confined_process_overwriting_the_guidance_in_its_context_dir",
    );
    assert_eq!(
        std::fs::read_to_string(&guidance).expect("guidance file must survive"),
        AGENT_GUIDANCE_TEXT,
        "the agent must not be able to rewrite {}",
        guidance.display()
    );
}

/// **seatbelt_still_lets_a_confined_process_write_beside_the_context_dir**: the carve-out is
/// surgical — the rest of the project root the context dir sits in stays writable, so the deny
/// cannot be mistaken for a blanket loss of the agent's working tree.
#[test]
fn seatbelt_still_lets_a_confined_process_write_beside_the_context_dir() {
    // Given
    let tree = a_project_tree_outside_the_os_scratch_base("tddy-context-deny-sibling");
    let project_root = tree.join("project");
    let egress = tree.join("egress");
    let context_dir = a_context_dir_holding_agent_guidance(&project_root);
    let sibling = project_root.join("scratch-note.txt");
    let plan = strict_system_plan_declaring_a_context_dir(
        &project_root,
        &egress,
        format!("echo written > '{}'", sibling.display()),
        &context_dir,
    );

    // When
    let mut handle = tddy_sandbox_darwin::spawn_plan(plan).expect("sandbox spawn must succeed");
    let exit = handle
        .child_mut()
        .wait()
        .expect("wait for sandbox child")
        .code()
        .unwrap_or(1);

    // Then
    assert_sandbox_exit(
        &egress,
        exit,
        true,
        "seatbelt_still_lets_a_confined_process_write_beside_the_context_dir",
    );
    assert_eq!(
        std::fs::read_to_string(&sibling).expect("sibling file must be created"),
        "written\n",
        "the project root beside the context dir must stay writable: {}",
        sibling.display()
    );
}
