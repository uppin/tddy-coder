//! The **home worktree** of a project's project-home session (`#project-home` 4/7): a dedicated
//! checkout, detached at the project's main branch ref, at `<worktrees root>/<project_id>/home`.
//! It is isolated from whatever branch the project's main checkout has, and is fast-forwarded to
//! the fetched ref before the home session resumes.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md.

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
    // TODO(home-session): implement
    let _ = (main_repo, worktrees_root, project_id, main_branch_ref);
    Err(HomeWorktreeError::Git(
        "ensure_home_worktree is not implemented".to_string(),
    ))
}

/// Fetch `main_branch_ref` and move the detached home worktree to it. Only ever a fast-forward:
/// the home worktree has no commits of its own, so anything else is an error, not a reset.
pub fn fast_forward_home_worktree(
    home: &Path,
    main_branch_ref: &str,
) -> Result<(), HomeWorktreeError> {
    // TODO(home-session): implement
    let _ = (home, main_branch_ref);
    Err(HomeWorktreeError::Git(
        "fast_forward_home_worktree is not implemented".to_string(),
    ))
}
