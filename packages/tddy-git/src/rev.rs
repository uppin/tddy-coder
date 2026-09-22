//! `git rev-parse` wrappers: resolved revisions and checked-out branch names.

use std::path::Path;
use std::process::Command;

pub(crate) fn git_rev_parse(cwd: &Path, rev: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse {} in {} failed: {}",
            rev,
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Short branch name for `rev` (e.g. `feature/a`), for comparison with [`git_head_branch_name`].
pub(crate) fn git_rev_parse_abbrev_ref(cwd: &Path, rev: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", rev])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse --abbrev-ref: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse --abbrev-ref {} in {} failed: {}",
            rev,
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s == "HEAD" {
        return Err(format!(
            "git rev-parse --abbrev-ref {} in {} resolved to detached HEAD",
            rev,
            cwd.display()
        ));
    }
    Ok(s)
}

/// The branch `worktree_dir` actually has checked out, or [`None`] when its `HEAD` is detached.
///
/// [`find_existing_worktree_for_branch_ref`](crate::find_existing_worktree_for_branch_ref) may answer with a worktree that merely *shares* the
/// branch's tip commit (its tier 2), which is fine for a caller that only displays an indicator and
/// wrong for one that is about to write. A mutation asks this first and refuses on a mismatch.
pub fn checked_out_branch_name(worktree_dir: &Path) -> Result<Option<String>, String> {
    git_head_branch_name(worktree_dir)
}

/// Current branch name in `cwd`, or [`None`] when `HEAD` is detached.
pub(crate) fn git_head_branch_name(cwd: &Path) -> Result<Option<String>, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse --abbrev-ref HEAD: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse --abbrev-ref HEAD in {} failed: {}",
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s == "HEAD" {
        return Ok(None);
    }
    Ok(Some(s))
}
