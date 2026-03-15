//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::changeset::{read_changeset, write_changeset};

/// Path to the worktrees directory under repo root.
pub fn worktree_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees")
}

/// Fetch origin/master. Must succeed before creating worktree from origin/master.
pub fn fetch_origin_master(repo_root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["fetch", "origin", "master"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git fetch origin master failed: {}", stderr));
    }
    Ok(())
}

/// Create a new git worktree. Returns the absolute path to the worktree.
///
/// When `start_point` is `Some("origin/master")`, creates the branch from that ref.
/// Otherwise uses HEAD.
pub fn create_worktree(
    repo_root: &Path,
    name: &str,
    branch: &str,
    start_point: Option<&str>,
) -> Result<PathBuf, String> {
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;

    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        return Err(format!(
            "worktree already exists: {}",
            worktree_path.display()
        ));
    }

    let mut args = vec![
        "worktree",
        "add",
        worktree_path.to_str().unwrap(),
        "-b",
        branch,
    ];
    if let Some(sp) = start_point {
        args.push(sp);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add failed: {}", stderr));
    }

    Ok(worktree_path.canonicalize().unwrap_or(worktree_path))
}

/// Create worktree for a session from origin/master. Fetches first, then creates,
/// updates changeset with worktree, branch, repo_path. Returns the worktree path.
pub fn setup_worktree_for_session(repo_root: &Path, plan_dir: &Path) -> Result<PathBuf, String> {
    let mut cs = read_changeset(plan_dir).map_err(|e| e.to_string())?;

    let branch = cs
        .branch_suggestion
        .clone()
        .or(cs.branch.clone())
        .or_else(|| {
            cs.name
                .as_ref()
                .map(|n| format!("feature/{}", slugify_for_branch(n)))
        })
        .ok_or("no branch suggestion or name for worktree")?;

    let worktree_name = cs
        .worktree_suggestion
        .clone()
        .or_else(|| cs.name.as_ref().map(|n| slugify_for_worktree(n)))
        .ok_or("no worktree suggestion or name for worktree")?;

    fetch_origin_master(repo_root)?;
    let worktree_path = create_worktree(repo_root, &worktree_name, &branch, Some("origin/master"))?;

    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
    cs.branch = Some(branch);
    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
    write_changeset(plan_dir, &cs).map_err(|e| e.to_string())?;

    Ok(worktree_path)
}

/// Remove an existing worktree. Uses `git worktree remove --force`.
pub fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap_or(""),
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree remove: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If git worktree remove fails (e.g. not registered), fall back to removing the directory
        log::debug!(
            "git worktree remove failed ({}), removing directory directly",
            stderr.trim()
        );
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path)
                .map_err(|e| format!("remove worktree dir: {}", e))?;
        }
    }
    Ok(())
}

fn slugify_for_branch(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn slugify_for_worktree(name: &str) -> String {
    slugify_for_branch(name)
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
