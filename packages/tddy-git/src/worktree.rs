//! Linked worktrees under `.worktrees/`: create, reuse, find, list and remove.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::rev::{git_head_branch_name, git_rev_parse, git_rev_parse_abbrev_ref};

/// Path to the worktrees directory under repo root.
pub fn worktree_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees")
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
    log::debug!(
        "create_worktree: repo={} name={} branch={} start_point={:?}",
        repo_root.display(),
        name,
        branch,
        start_point
    );
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;

    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        return Err(format!(
            "worktree path already exists at {} — reuse the existing worktree or confirm before proceeding",
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

/// If `worktree_path` exists and is already a linked worktree of `repo_root`, and its `HEAD`
/// matches `git rev-parse <branch>` in `repo_root`, return that path for resume (changeset lost
/// `worktree` but the directory remains registered with Git).
fn try_reuse_linked_worktree_at_path(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
) -> Result<Option<PathBuf>, String> {
    if !worktree_path.exists() {
        return Ok(None);
    }
    if !path_is_registered_worktree_of_repo(repo_root, worktree_path)? {
        return Ok(None);
    }
    let expected = git_rev_parse(repo_root, branch)?;
    let actual = git_rev_parse(worktree_path, "HEAD")?;
    if expected != actual {
        return Err(format!(
            "existing worktree at {} has HEAD {actual} but {branch} resolves to {expected}; \
             remove the directory or fix the worktree before retrying",
            worktree_path.display()
        ));
    }
    Ok(Some(
        worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.to_path_buf()),
    ))
}

fn path_is_registered_worktree_of_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<bool, String> {
    let want = worktree_path
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {}", worktree_path.display(), e))?;
    let out = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        let p = PathBuf::from(rest.trim());
        let canon = p.canonicalize().unwrap_or(p);
        if canon == want {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Lists absolute paths of all registered worktrees (including the primary checkout).
fn registered_worktree_paths(repo_root: &Path) -> Result<Vec<PathBuf>, String> {
    let out = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut paths = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        paths.push(PathBuf::from(rest.trim()));
    }
    Ok(paths)
}

/// Finds a worktree to reuse for `branch_ref`:
/// 1. **Name match**: a registered worktree whose current branch name equals `git rev-parse
///    --abbrev-ref` of `branch_ref` in `repo_root` (e.g. `feature/x` checked out for `feature/x`).
/// 2. Else **linked + same tip**: a worktree path under **`.worktrees/`** whose `HEAD` equals the
///    resolved commit of `branch_ref`. This covers `origin/feature/x` vs a local `feature/x`
///    checkout without matching the **primary** checkout when it sits on another branch (e.g.
///    `master`) while an unused local branch exists at the same commit as `master` — the primary
///    is not under `.worktrees/` and is excluded from tier 2.
///
/// Preference order within a tier: paths under **`.worktrees/`** first, then others, then
/// lexicographic path order for stability.
pub fn find_existing_worktree_for_branch_ref(
    repo_root: &Path,
    branch_ref: &str,
) -> Result<Option<PathBuf>, String> {
    let target = git_rev_parse(repo_root, branch_ref)?;
    let want_branch = git_rev_parse_abbrev_ref(repo_root, branch_ref).ok();

    let mut by_name: Vec<PathBuf> = Vec::new();
    let mut by_commit_linked: Vec<PathBuf> = Vec::new();

    for p in registered_worktree_paths(repo_root)? {
        if !p.exists() {
            continue;
        }
        if let Some(ref w) = want_branch {
            if let Some(cur) = git_head_branch_name(&p)? {
                if cur == *w {
                    by_name.push(p.canonicalize().unwrap_or(p));
                    continue;
                }
            }
        }
        let Ok(head) = git_rev_parse(&p, "HEAD") else {
            continue;
        };
        if head != target {
            continue;
        }
        if p.to_string_lossy().contains("/.worktrees/") {
            by_commit_linked.push(p.canonicalize().unwrap_or(p));
        }
    }

    let sort_key = |a: &PathBuf, b: &PathBuf| {
        let aw = a.to_string_lossy().contains("/.worktrees/");
        let bw = b.to_string_lossy().contains("/.worktrees/");
        bw.cmp(&aw).then_with(|| a.cmp(b))
    };

    if !by_name.is_empty() {
        let mut v = by_name;
        v.sort_by(sort_key);
        return Ok(v.into_iter().next());
    }
    if !by_commit_linked.is_empty() {
        let mut v = by_commit_linked;
        v.sort_by(sort_key);
        return Ok(v.into_iter().next());
    }
    Ok(None)
}

/// Public, non-erroring wrapper over [`try_find_existing_worktree_for_branch_ref`]: returns the
/// on-disk worktree path checked out for `branch` in `repo_root`, or `None` when the branch does not
/// resolve or has no worktree. Errors (I/O, worktree enumeration) collapse to `None` — callers that
/// only want to display a worktree indicator do not need to distinguish "no worktree" from a
/// transient git error.
pub fn worktree_path_for_branch(repo_root: &Path, branch: &str) -> Option<PathBuf> {
    try_find_existing_worktree_for_branch_ref(repo_root, branch)
        .ok()
        .flatten()
}

/// Like [`find_existing_worktree_for_branch_ref`], but returns `Ok(None)` when `branch_ref` does not
/// resolve in `repo_root` (e.g. suggested branch not created yet). Propagates I/O and worktree
/// enumeration errors.
pub fn try_find_existing_worktree_for_branch_ref(
    repo_root: &Path,
    branch_ref: &str,
) -> Result<Option<PathBuf>, String> {
    let verify = Command::new("git")
        .args(["rev-parse", "--verify", branch_ref])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse --verify: {}", e))?;
    if !verify.status.success() {
        return Ok(None);
    }
    find_existing_worktree_for_branch_ref(repo_root, branch_ref)
}

/// Add a linked worktree at `.worktrees/<name>` checked out to an **existing** local branch.
///
/// Uses `git worktree add <path> <branch>` when the branch is not already checked out in another
/// worktree. If Git refuses because the branch is in use (common when the primary repo already
/// has `main` checked out), falls back to `worktree add --detach` at the branch tip, then
/// `git switch --ignore-other-worktrees <branch>` in the new worktree so `branch --show-current`
/// matches the selected branch (PRD: work on selected branch).
///
/// When the path already exists, the error instructs the user to confirm reuse (PRD).
pub fn add_worktree_for_existing_branch(
    repo_root: &Path,
    name: &str,
    branch: &str,
) -> Result<PathBuf, String> {
    log::info!(
        "add_worktree_for_existing_branch: repo={} worktree_name={} branch={}",
        repo_root.display(),
        name,
        branch
    );
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;
    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        match try_reuse_linked_worktree_at_path(repo_root, &worktree_path, branch) {
            Ok(Some(p)) => {
                log::info!(
                    "add_worktree_for_existing_branch: reusing existing linked worktree at {}",
                    p.display()
                );
                return Ok(p);
            }
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        return Err(format!(
            "worktree path already exists at {} — reuse the existing worktree or confirm before proceeding",
            worktree_path.display()
        ));
    }

    let try_direct = Command::new("git")
        .args(["worktree", "add", worktree_path.to_str().unwrap(), branch])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add: {}", e))?;

    if try_direct.status.success() {
        return Ok(worktree_path.canonicalize().unwrap_or(worktree_path));
    }

    let stderr = String::from_utf8_lossy(&try_direct.stderr);
    log::debug!(
        "add_worktree_for_existing_branch: direct add failed stderr={}",
        stderr.trim()
    );

    let branch_in_use = stderr.contains("already used")
        || stderr.contains("is already checked out")
        || stderr.to_lowercase().contains("already");

    if !branch_in_use {
        return Err(format!("git worktree add failed: {}", stderr));
    }

    log::info!(
        "add_worktree_for_existing_branch: using detach+switch fallback for branch {}",
        branch
    );

    let rev_out = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !rev_out.status.success() {
        let rev_stderr = String::from_utf8_lossy(&rev_out.stderr);
        return Err(format!(
            "git rev-parse refs/heads/{branch} failed: {}",
            rev_stderr
        ));
    }
    let rev = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

    let detach = Command::new("git")
        .args([
            "worktree",
            "add",
            "--detach",
            worktree_path.to_str().unwrap(),
            &rev,
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add --detach: {}", e))?;
    if !detach.status.success() {
        let e = String::from_utf8_lossy(&detach.stderr);
        return Err(format!("git worktree add --detach failed: {}", e));
    }

    let sw = Command::new("git")
        .args(["switch", "--ignore-other-worktrees", branch])
        .current_dir(&worktree_path)
        .output()
        .map_err(|e| format!("git switch: {}", e))?;
    if !sw.status.success() {
        let e = String::from_utf8_lossy(&sw.stderr);
        return Err(format!(
            "git switch --ignore-other-worktrees {branch} failed: {}",
            e
        ));
    }

    Ok(worktree_path.canonicalize().unwrap_or(worktree_path))
}

const MAX_WORKTREE_RETRIES: u32 = 20;

/// Try `create_worktree`; on "branch ... already exists" retry with `-1`, `-2`, etc.
/// Returns `(worktree_path, actual_branch_name)`.
pub fn create_worktree_with_retry(
    repo_root: &Path,
    name: &str,
    branch: &str,
    start_point: Option<&str>,
) -> Result<(PathBuf, String), String> {
    match create_worktree(repo_root, name, branch, start_point) {
        Ok(path) => return Ok((path, branch.to_string())),
        Err(e) if e.contains("already exists") => {
            log::debug!("worktree branch {branch:?} exists, retrying with suffix");
        }
        Err(e) => return Err(e),
    }
    for i in 1..=MAX_WORKTREE_RETRIES {
        let suffixed_branch = format!("{branch}-{i}");
        let suffixed_name = format!("{name}-{i}");
        match create_worktree(repo_root, &suffixed_name, &suffixed_branch, start_point) {
            Ok(path) => return Ok((path, suffixed_branch)),
            Err(e) if e.contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    Err(format!(
        "exhausted {MAX_WORKTREE_RETRIES} retries for branch {branch:?}"
    ))
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
