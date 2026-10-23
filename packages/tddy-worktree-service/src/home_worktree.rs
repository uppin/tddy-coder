//! The **home worktree** of a project's project-home session (`#project-home` 4/7): a dedicated
//! checkout, detached at the project's main branch ref, at `<worktrees root>/<project_id>/home`.
//! It is isolated from whatever branch the project's main checkout has, and is fast-forwarded to
//! the fetched ref before the home session resumes.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use std::fmt;
use std::path::{Path, PathBuf};

/// Why a home worktree could not be created or brought current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeWorktreeError {
    /// `main_branch_ref` does not resolve in the main repository after fetching it.
    UnknownRef(String),
    /// A path exists where the home worktree belongs, and it is not that worktree.
    PathTaken(PathBuf),
    /// A git command failed; the message is git's.
    Git(String),
}

impl fmt::Display for HomeWorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRef(r) => write!(f, "main branch ref {r} does not resolve"),
            Self::PathTaken(p) => write!(f, "{} exists and is not the home worktree", p.display()),
            Self::Git(msg) => write!(f, "git failed: {msg}"),
        }
    }
}

impl std::error::Error for HomeWorktreeError {}

/// The home worktree's path for `project_id` under `worktrees_root`.
pub fn home_worktree_path(worktrees_root: &Path, project_id: &str) -> PathBuf {
    worktrees_root.join(project_id).join("home")
}

/// Create the home worktree (detached at the fetched `main_branch_ref`) unless it already exists,
/// and return its path. An existing home worktree is returned untouched.
pub fn ensure_home_worktree(
    main_repo: &Path,
    worktrees_root: &Path,
    project_id: &str,
    main_branch_ref: &str,
) -> Result<PathBuf, HomeWorktreeError> {
    let home = home_worktree_path(worktrees_root, project_id);
    if home.exists() {
        return if is_linked_worktree_of(main_repo, &home)? {
            Ok(home)
        } else {
            Err(HomeWorktreeError::PathTaken(home))
        };
    }
    fetch_ref(main_repo, main_branch_ref)?;
    resolve_ref(main_repo, main_branch_ref)?;
    if let Some(parent) = home.parent() {
        std::fs::create_dir_all(parent).map_err(|e| HomeWorktreeError::Git(e.to_string()))?;
    }
    let home_arg = home.to_string_lossy().into_owned();
    git(
        main_repo,
        &["worktree", "add", "--detach", &home_arg, main_branch_ref],
    )?;
    Ok(home)
}

/// Fetch `main_branch_ref` and move the detached home worktree to it. Only ever a fast-forward:
/// the home worktree has no commits of its own, so anything else is an error, not a reset.
pub fn fast_forward_home_worktree(
    home: &Path,
    main_branch_ref: &str,
) -> Result<(), HomeWorktreeError> {
    fetch_ref(home, main_branch_ref)?;
    resolve_ref(home, main_branch_ref)?;
    git(home, &["merge", "--ff-only", main_branch_ref]).map(|_| ())
}

/// Whether `path` is one of `main_repo`'s linked worktrees.
fn is_linked_worktree_of(main_repo: &Path, path: &Path) -> Result<bool, HomeWorktreeError> {
    let listing = git(main_repo, &["worktree", "list", "--porcelain"])?;
    let wanted = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Ok(listing
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|listed| {
            std::fs::canonicalize(listed).unwrap_or_else(|_| PathBuf::from(listed)) == wanted
        }))
}

/// Fetch the branch `main_branch_ref` (`<remote>/<branch>`) names from its remote. A ref whose
/// first segment is no remote of this repository (a local branch) has nothing to fetch.
fn fetch_ref(repo: &Path, main_branch_ref: &str) -> Result<(), HomeWorktreeError> {
    let Some((remote, branch)) = main_branch_ref.split_once('/') else {
        return Ok(());
    };
    let remotes = git(repo, &["remote"])?;
    if !remotes.lines().any(|r| r == remote) {
        return Ok(());
    }
    git(repo, &["fetch", "--quiet", remote, branch]).map(|_| ())
}

fn resolve_ref(repo: &Path, main_branch_ref: &str) -> Result<(), HomeWorktreeError> {
    let spec = format!("{main_branch_ref}^{{commit}}");
    git(repo, &["rev-parse", "--verify", "--quiet", &spec])
        .map(|_| ())
        .map_err(|_| HomeWorktreeError::UnknownRef(main_branch_ref.to_string()))
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, HomeWorktreeError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| HomeWorktreeError::Git(format!("git {}: {e}", args.join(" "))))?;
    if !output.status.success() {
        return Err(HomeWorktreeError::Git(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
