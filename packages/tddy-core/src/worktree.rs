//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the worktrees directory under repo root.
pub fn worktree_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees")
}

/// Create a new git worktree. Returns the absolute path to the worktree.
///
/// Runs `git worktree add .worktrees/<name> -b <branch>`.
pub fn create_worktree(repo_root: &Path, name: &str, branch: &str) -> Result<PathBuf, String> {
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;

    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        return Err(format!(
            "worktree already exists: {}",
            worktree_path.display()
        ));
    }

    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            branch,
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add failed: {}", stderr));
    }

    Ok(worktree_path.canonicalize().unwrap_or(worktree_path))
}

/// Info about an existing worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
}

/// List worktrees under the repo. Returns the main worktree and any linked worktrees.
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let (Some(p), b) = (current_path.take(), current_branch.take()) {
                worktrees.push(WorktreeInfo { path: p, branch: b });
            }
            current_path = Some(PathBuf::from(path.trim()));
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.trim().to_string());
        }
    }
    if let Some(p) = current_path {
        worktrees.push(WorktreeInfo {
            path: p,
            branch: current_branch,
        });
    }

    Ok(worktrees)
}
