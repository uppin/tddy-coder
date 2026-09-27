//! A workspace jail can actually run the host's `ripgrep`.
//!
//! `tool_grep` shells out to `rg`, and until this was fixed every `Grep` call inside a sandboxed
//! session on a Mac returned `spawn failed: No such file or directory`. The fix is only meaningful
//! if Seatbelt really admits the binary, so this test renders the **real** profile
//! `build_workspace_tool_plan` produces and hands it to `sandbox-exec`, the same program the
//! spawner uses. Nothing is asserted about the grants' shape — only that the confined process
//! runs.
//!
//! Three things had to be right, and each of them failed a `sandbox-exec` probe on the way here:
//! `which` reports a symlink into `Cellar`, `rg` links a Homebrew `libpcre2`, and the loader asks
//! for that library through a symlink farm the profile renderer canonicalizes away.
//!
//! ## This runs on no CI machine
//!
//! Seatbelt is macOS, and every CI job is `runs-on: ubuntu-*`, so this compiles to an empty binary
//! there — the same gap `docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`
//! records for the in-jail conversation suite. It is written for a developer's Mac, where it is
//! the only thing that proves the fix.
//!
//! Changeset: PR #547; replaced by the declared tool set in PR #548.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    build_workspace_tool_plan, WorkspaceSandboxLayout, WorkspaceToolPlanRequest,
};

/// The host's `rg`, resolved the way the jail resolves it.
///
/// The plan puts the **canonical** binary's directory on the jail's `PATH`, so `Command::new("rg")`
/// inside the jail resolves to the real file rather than to the `/opt/homebrew/bin` symlink. This
/// test names that same path directly, because `sandbox-exec` inherits this process's environment
/// rather than the plan's — handing it the symlink would test a path production never takes, and
/// Seatbelt denies the symlink spelling even where it admits what it points at.
fn the_hosts_ripgrep() -> PathBuf {
    let out = Command::new("/usr/bin/which")
        .arg("rg")
        .output()
        .expect("`which` must run");
    assert!(
        out.status.success(),
        "this test needs ripgrep on PATH — `brew install ripgrep`"
    );
    let reported = PathBuf::from(String::from_utf8(out.stdout).expect("a path").trim());
    std::fs::canonicalize(&reported).expect("ripgrep must resolve")
}

/// A jail plan for a session whose checkout is `worktree`, rendered to a Seatbelt profile on disk.
fn a_rendered_profile_for(session_dir: &Path, worktree: &Path) -> PathBuf {
    let layout = WorkspaceSandboxLayout::under_session_dir(session_dir);
    for dir in [
        &layout.sandbox_root,
        &layout.scratch_dir.join("home"),
        &layout.scratch_dir.join("tmp"),
        &layout.context_dir,
        &layout.egress_dir,
    ] {
        std::fs::create_dir_all(dir).expect("the jail tree");
    }
    let plan = build_workspace_tool_plan(WorkspaceToolPlanRequest {
        layout: layout.clone(),
        worktree_path: worktree.to_path_buf(),
        session_id: "ripgrep-in-jail".to_string(),
        runner_path: "/usr/bin/true".to_string(),
        tddy_tools_path: "/usr/bin/true".to_string(),
        cgroup: Default::default(),
    })
    .expect("the plan must build");

    let profile = tddy_sandbox_darwin::render_plan(&plan).expect("the profile must render");
    let path = layout.sandbox_root.join("probe.sb");
    std::fs::write(&path, profile).expect("the profile must be written");
    path
}

#[test]
fn a_workspace_jail_can_run_the_hosts_ripgrep() {
    // Given a rendered profile for a real jail, and a file in the checkout to search
    let session = tempfile::tempdir().expect("a session dir");
    let worktree = tempfile::tempdir().expect("a worktree");
    std::fs::write(worktree.path().join("hay.rs"), "const NEEDLE: u32 = 1;\n")
        .expect("something to find");
    // `JailedWorkspaceSandboxProvisioner::provision` canonicalizes the worktree before building
    // the plan, because Seatbelt matches resolved paths and `/tmp` is a symlink to `/private/tmp`.
    // Skip that and the mount names a path the kernel never reports.
    let worktree_path = std::fs::canonicalize(worktree.path()).expect("a canonical worktree");
    let profile = a_rendered_profile_for(session.path(), &worktree_path);

    // When ripgrep runs under it, confined exactly as a session's tools are
    let out = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg(the_hosts_ripgrep())
        .arg("--no-config")
        .arg("NEEDLE")
        .arg(&worktree_path)
        .output()
        .expect("sandbox-exec must run");

    // Then it found the line, rather than being denied before it started
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("const NEEDLE"),
        "ripgrep did not search from inside the jail.\nstdout: {stdout}\nstderr: {stderr}"
    );
}
