//! Git operations for **merge-pr** (fetch `origin`, merge default branch into feature, push).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::review::resolve_git_repo_root;

/// Configuration for sync (session worktree path, remote name, default branch).
#[derive(Debug, Clone, Default)]
pub struct MergePrGitConfig {
    /// Session / worktree root; git state is read from here.
    pub session_worktree: Option<PathBuf>,
    /// Remote name (default **`origin`**).
    pub remote_name: Option<String>,
    /// Default integration branch on the remote (default **`main`**).
    pub default_branch: Option<String>,
}

/// Outcome of a successful sync (for structured reporting / tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePrSyncReport {
    pub strategy: &'static str,
}

fn run_git(repo: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| {
            format!(
                "failed to run git {} in {}: {e}",
                args.join(" "),
                repo.display()
            )
        })
}

/// Fetch `origin`, merge **`origin/main`** (or configured remote/branch) into `HEAD`.
pub fn sync_feature_with_origin_main(
    config: &MergePrGitConfig,
) -> Result<MergePrSyncReport, String> {
    let Some(root) = config.session_worktree.as_ref() else {
        return Err(
            "merge-pr: session worktree path is not set; sync requires a session worktree"
                .to_string(),
        );
    };

    if !root.exists() {
        return Err(format!(
            "merge-pr: session worktree does not exist: {}",
            root.display()
        ));
    }

    let repo_root = resolve_git_repo_root(root).ok_or_else(|| {
        format!(
            "merge-pr: not a git repository (no .git under {}); cannot sync",
            root.display()
        )
    })?;

    let remote = config.remote_name.as_deref().unwrap_or("origin");
    let branch = config.default_branch.as_deref().unwrap_or("main");
    let merge_ref = format!("{remote}/{branch}");

    let fetch = run_git(&repo_root, &["fetch", remote])?;
    if !fetch.status.success() {
        return Err(format!(
            "git fetch {remote} failed: {}",
            String::from_utf8_lossy(&fetch.stderr)
        ));
    }

    let merge = run_git(&repo_root, &["merge", &merge_ref, "--no-edit"])?;
    if merge.status.success() {
        return Ok(MergePrSyncReport { strategy: "merge" });
    }

    let ls = run_git(&repo_root, &["ls-files", "-u"])?;
    if !ls.status.success() {
        return Err(format!(
            "git ls-files -u failed after merge attempt: {}",
            String::from_utf8_lossy(&ls.stderr)
        ));
    }

    let unmerged = String::from_utf8_lossy(&ls.stdout);
    if !unmerged.trim().is_empty() {
        return Err(format!(
            "merge-pr: merge conflicts while merging {merge_ref} into the current branch; \
             resolve conflicts in the worktree, then retry. \
             git reported: {}",
            String::from_utf8_lossy(&merge.stderr).trim()
        ));
    }

    Err(format!(
        "git merge {merge_ref} failed: {}",
        String::from_utf8_lossy(&merge.stderr)
    ))
}

/// Fail closed when the index still has unmerged paths.
pub fn ensure_no_unmerged_paths(repo_root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["ls-files", "-u"])
        .output()
        .map_err(|e| format!("git ls-files -u: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("not a git repository") {
            return Ok(());
        }
        return Err(format!("git ls-files -u failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.trim().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "merge-pr: index has unmerged paths; resolve conflicts before continuing:\n{}",
            stdout.trim()
        ))
    }
}
