//! The files one headless turn writes — its MCP registration and its session pointer — and their
//! removal when the turn ends (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! A file under the agent's working directory (Cursor's `.cursor/mcp.json`, the
//! [`crate::session_pointer::SESSION_POINTER_FILE`]) is git-excluded through the repository's
//! `info/exclude` before it is written, so it can never land in a commit; a working directory that
//! is not a git checkout is refused, since nothing would keep the file out of one.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::BackendError;
use crate::session_pointer::{SessionPointer, SESSION_POINTER_FILE};

/// The mode a turn file is written with: an MCP registration can carry the agent's tool credential.
const OWNER_ONLY_FILE: u32 = 0o600;

/// The files a turn wrote. Dropping it removes every one of them, however the turn ended.
#[derive(Debug, Default)]
pub struct TurnFiles {
    written: Vec<PathBuf>,
    /// Directories under the worktree this turn created, removed (when empty) after its files.
    created_dirs: Vec<PathBuf>,
}

impl TurnFiles {
    /// Writes `files` owner-only (creating their directories), git-excluding each that lies under
    /// `workdir`, and returns the guard that removes them.
    pub fn write(files: &[(PathBuf, Vec<u8>)], workdir: &Path) -> Result<Self, BackendError> {
        let mut turn_files = Self::default();
        for (path, contents) in files {
            turn_files.write_one(path, contents, workdir)?;
        }
        Ok(turn_files)
    }

    /// Also writes `pointer` as `<workdir>/`[`crate::session_pointer::SESSION_POINTER_FILE`],
    /// git-excluded, removed with the rest.
    pub fn write_session_pointer(
        &mut self,
        workdir: &Path,
        pointer: &SessionPointer,
    ) -> Result<(), BackendError> {
        let path = workdir.join(SESSION_POINTER_FILE);
        let contents = serde_json::to_vec_pretty(pointer).map_err(|e| {
            BackendError::InvocationFailed(format!("the session pointer cannot be encoded: {e}"))
        })?;
        // The pointer is this backend's own file: a stale one a crashed turn left is replaced.
        self.write_one_replacing(&path, &contents, workdir)
    }

    fn write_one(
        &mut self,
        path: &Path,
        contents: &[u8],
        workdir: &Path,
    ) -> Result<(), BackendError> {
        // A file in the worktree may be the repository's own, and tracked: a turn that wrote over it
        // and removed it afterwards would destroy it.
        if path.starts_with(workdir) && path.symlink_metadata().is_ok() {
            return Err(BackendError::InvocationFailed(format!(
                "{} already exists in the worktree, and is not overwritten",
                path.display()
            )));
        }
        self.write_one_replacing(path, contents, workdir)
    }

    fn write_one_replacing(
        &mut self,
        path: &Path,
        contents: &[u8],
        workdir: &Path,
    ) -> Result<(), BackendError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| path.starts_with(workdir) && *parent != workdir && !parent.exists())
        {
            self.created_dirs.push(parent.to_path_buf());
        }
        if let Ok(relative) = path.strip_prefix(workdir) {
            // Excluded before it is written, so it is never an untracked file git would offer.
            exclude_from_git(workdir, &format!("/{}", relative.to_string_lossy()))?;
        }
        crate::atomic_file::write_atomic_with_mode(path, contents, OWNER_ONLY_FILE).map_err(
            |e| BackendError::InvocationFailed(format!("could not write {}: {e}", path.display())),
        )?;
        self.written.push(path.to_path_buf());
        Ok(())
    }

    /// The files written, in order.
    pub fn paths(&self) -> &[PathBuf] {
        &self.written
    }
}

impl Drop for TurnFiles {
    fn drop(&mut self) {
        for path in &self.written {
            if let Err(e) = std::fs::remove_file(path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("[turn-files] could not remove {}: {e}", path.display());
                }
            }
        }
        for dir in &self.created_dirs {
            // Fails, harmlessly, when something else has been put in it since.
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Adds `pattern` (`/.cursor/mcp.json`) to the `info/exclude` of the repository `worktree` is a
/// checkout of, unless it is there already. Refused when `worktree` is not a git checkout.
pub fn exclude_from_git(worktree: &Path, pattern: &str) -> Result<(), BackendError> {
    let not_a_checkout = |detail: String| {
        BackendError::InvocationFailed(format!(
            "its worktree {} is not a git checkout, so nothing would keep {pattern} out of a commit: {detail}",
            worktree.display()
        ))
    };
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .map_err(|e| {
            BackendError::InvocationFailed(format!(
                "git could not be run to keep {pattern} out of commits: {e}"
            ))
        })?;
    if !output.status.success() {
        return Err(not_a_checkout(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let common_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let exclude = common_dir.join("info").join("exclude");
    let io_error = |e: std::io::Error| {
        BackendError::InvocationFailed(format!("{} could not be updated: {e}", exclude.display()))
    };
    let existing = match std::fs::read_to_string(&exclude) {
        Ok(existing) => existing,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(io_error(e)),
    };
    if existing.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    std::fs::create_dir_all(common_dir.join("info")).map_err(io_error)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)
        .and_then(|mut file| file.write_all(format!("{separator}{pattern}\n").as_bytes()))
        .map_err(io_error)
}
