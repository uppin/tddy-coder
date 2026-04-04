//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::changeset::{read_changeset, write_changeset};

/// Default remote-tracking ref used for integration worktrees when a project does not specify
/// `main_branch_ref` in the daemon project registry (legacy YAML rows).
///
/// This matches the historical hardcoded contract (`origin/master`) before per-project base refs.
pub const DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF: &str = "origin/master";

/// Validates a per-project integration base ref: a single remote-tracking ref `origin/<branch>` with
/// no shell metacharacters or extra git arguments.
pub fn validate_integration_base_ref(s: &str) -> Result<(), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("integration base ref must not be empty".to_string());
    }
    let rest = s
        .strip_prefix("origin/")
        .ok_or_else(|| "integration base ref must start with origin/".to_string())?;
    if rest.is_empty() {
        return Err("integration base ref must be origin/<branch-name>".to_string());
    }
    if rest.contains('/') {
        return Err(
            "integration base ref must be a single remote branch segment: origin/<branch-name>"
                .to_string(),
        );
    }
    if rest.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref must not contain whitespace".to_string());
    }
    for forbidden in [';', '|', '&', '$', '`', '\n', '\r'] {
        if rest.contains(forbidden) {
            return Err(format!(
                "integration base ref contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    if rest.contains("--") {
        return Err("integration base ref must not contain `--`".to_string());
    }
    Ok(())
}

/// Path to the worktrees directory under repo root.
pub fn worktree_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees")
}

/// Fetch origin/master. Must succeed before creating worktree from origin/master.
pub fn fetch_origin_master(repo_root: &Path) -> Result<(), String> {
    log::debug!("fetch_origin_master: repo_root={}", repo_root.display());
    fetch_integration_base(repo_root, DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF)
}

/// Fetches the given remote-tracking integration base ref (e.g. `origin/main`).
pub fn fetch_integration_base(repo_root: &Path, integration_base_ref: &str) -> Result<(), String> {
    validate_integration_base_ref(integration_base_ref)?;
    let branch = integration_base_ref
        .strip_prefix("origin/")
        .expect("validate_integration_base_ref ensures origin/ prefix");
    log::info!(
        "fetch_integration_base: repo={} integration_base_ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let output = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch origin {}: {}", branch, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "fetch_integration_base: git fetch failed stderr={}",
            stderr.trim()
        );
        return Err(format!("git fetch origin {} failed: {}", branch, stderr));
    }
    log::debug!(
        "fetch_integration_base: fetch completed for {}",
        integration_base_ref
    );
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

const MAX_WORKTREE_RETRIES: u32 = 20;

/// Try `create_worktree`; on "branch ... already exists" retry with `-1`, `-2`, etc.
/// Returns `(worktree_path, actual_branch_name)`.
fn create_worktree_with_retry(
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

/// Create worktree for a session using an explicit integration base ref (e.g. `origin/main`).
pub fn setup_worktree_for_session_with_integration_base(
    repo_root: &Path,
    session_dir: &Path,
    integration_base_ref: &str,
) -> Result<PathBuf, String> {
    validate_integration_base_ref(integration_base_ref)?;
    log::info!(
        "setup_worktree_for_session_with_integration_base: repo={} ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let mut cs = read_changeset(session_dir).map_err(|e| e.to_string())?;

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

    fetch_integration_base(repo_root, integration_base_ref)?;

    let (worktree_path, actual_branch) = create_worktree_with_retry(
        repo_root,
        &worktree_name,
        &branch,
        Some(integration_base_ref),
    )?;

    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
    cs.branch = Some(actual_branch);
    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;

    log::debug!(
        "setup_worktree_for_session_with_integration_base: worktree_path={}",
        worktree_path.display()
    );
    Ok(worktree_path)
}

/// Resolves which remote-tracking ref to use when no per-project override is supplied.
///
/// Runs `git fetch origin`, then prefers `origin/master` when present (legacy default contract),
/// otherwise `origin/main` if present, otherwise follows `refs/remotes/origin/HEAD`.
pub fn resolve_default_integration_base_ref(repo_root: &Path) -> Result<String, String> {
    log::info!(
        "resolve_default_integration_base_ref: fetching origin repo={}",
        repo_root.display()
    );
    let fetch_out = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch origin: {}", e))?;
    if !fetch_out.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("git fetch origin failed: {}", stderr));
    }

    if remote_ref_exists(repo_root, "origin/master")? {
        log::debug!("resolve_default_integration_base_ref: chose origin/master");
        return Ok("origin/master".to_string());
    }
    if remote_ref_exists(repo_root, "origin/main")? {
        log::debug!("resolve_default_integration_base_ref: chose origin/main");
        return Ok("origin/main".to_string());
    }

    let sym = Command::new("git")
        .args(["symbolic-ref", "-q", "refs/remotes/origin/HEAD"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git symbolic-ref: {}", e))?;
    if sym.status.success() {
        let sym_ref = String::from_utf8_lossy(&sym.stdout).trim().to_string();
        log::debug!(
            "resolve_default_integration_base_ref: origin/HEAD -> {}",
            sym_ref
        );
        if let Some(rest) = sym_ref.strip_prefix("refs/remotes/") {
            validate_integration_base_ref(rest)?;
            return Ok(rest.to_string());
        }
    }

    Err(
        "could not resolve integration base ref: no origin/master, origin/main, or origin/HEAD"
            .to_string(),
    )
}

fn remote_ref_exists(repo_root: &Path, rev: &str) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    Ok(out.status.success())
}

/// Create worktree for a session. Fetches the resolved integration base, then creates,
/// updates changeset with worktree, branch, repo_path. Returns the worktree path.
///
/// When no project-specific ref is available, the ref is resolved with
/// [`resolve_default_integration_base_ref`] (prefers `origin/master`, then `origin/main`, then
/// `origin/HEAD`).
pub fn setup_worktree_for_session(repo_root: &Path, session_dir: &Path) -> Result<PathBuf, String> {
    log::info!(
        "setup_worktree_for_session: repo_root={}",
        repo_root.display()
    );
    let integration_base_ref = resolve_default_integration_base_ref(repo_root)?;
    setup_worktree_for_session_with_integration_base(repo_root, session_dir, &integration_base_ref)
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

#[cfg(test)]
mod integration_base_red_tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    /// `fetch_integration_base` runs `git fetch origin <branch>` for a valid remote-tracking ref.
    #[test]
    fn fetch_integration_base_succeeds_for_valid_origin_main_red() {
        let base = std::env::temp_dir().join("tddy-core-fetch-int-base-green");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("f"), "x").unwrap();
        Command::new("git")
            .args(["add", "f"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", repo.to_str().unwrap()])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        assert!(
            fetch_integration_base(&repo, "origin/main").is_ok(),
            "fetch_integration_base must succeed for a valid repo and ref"
        );
    }

    /// RED: session setup with explicit `origin/main` must complete worktree creation (skeleton returns Err).
    #[test]
    fn setup_worktree_with_integration_base_completes_red() {
        let base = std::env::temp_dir().join("tddy-core-setup-int-base-red");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("f"), "x").unwrap();
        Command::new("git")
            .args(["add", "f"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", repo.to_str().unwrap()])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let session_dir = base.join("sess");
        fs::create_dir_all(&session_dir).unwrap();
        let cs = crate::changeset::Changeset {
            name: Some("n".to_string()),
            branch_suggestion: Some("feature/x".to_string()),
            worktree_suggestion: Some("feature-x".to_string()),
            ..Default::default()
        };
        crate::changeset::write_changeset(&session_dir, &cs).unwrap();

        let r =
            setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main");
        assert!(
            r.is_ok(),
            "GREEN: must create worktree from origin/main; got {:?}",
            r.err()
        );
    }
}
