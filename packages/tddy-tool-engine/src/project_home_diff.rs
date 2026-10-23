//! The diff `DiffSessionWorktree` reads of another session's worktree (`#project-home` 5/7).
//!
//! `git diff <base>` alone leaves out untracked files, and a session's new file is the first thing
//! its project-home neighbour wants to see. So the diff is taken over a temporary index that holds
//! the worktree's whole state: `read-tree HEAD`, then `add -A` (which honours `.gitignore`), then
//! `diff --cached <base>`. The worktree's own index is never written.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;

/// The worktree's diff against `base_ref`, including its uncommitted and untracked files.
///
/// `base_ref` comes from the target session's own records, so one that git would read as an option
/// (a leading `-`) is refused rather than passed on.
pub(crate) async fn worktree_diff(worktree: &Path, base_ref: &str) -> Result<String, String> {
    if base_ref.starts_with('-') {
        return Err(format!("base ref {base_ref:?} is not a ref"));
    }
    let index = temp_index_path();
    let diff = diff_over_index(worktree, base_ref, &index).await;
    let _ = std::fs::remove_file(&index);
    diff
}

async fn diff_over_index(worktree: &Path, base_ref: &str, index: &Path) -> Result<String, String> {
    git(worktree, index, &["read-tree", "HEAD"]).await?;
    git(worktree, index, &["add", "-A"]).await?;
    git(
        worktree,
        index,
        &["diff", "--cached", "--no-color", base_ref, "--"],
    )
    .await
}

async fn git(worktree: &Path, index: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(args)
        .env("GIT_INDEX_FILE", index)
        .output()
        .await
        .map_err(|e| format!("cannot run git {}: {e}", args[0]))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn temp_index_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "tddy-home-diff-{}-{nanos}.index",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_base_ref_git_would_read_as_an_option_is_refused() {
        // Given a base ref a target session's record could carry
        let refused = worktree_diff(Path::new("."), "--output=/tmp/x")
            .await
            .expect_err("an option-shaped base ref is refused");

        // Then git is never asked, and the refusal names the ref
        assert!(refused.contains("--output=/tmp/x"), "{refused}");
    }
}
