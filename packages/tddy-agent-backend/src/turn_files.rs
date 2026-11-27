//! The files one headless turn writes — its MCP registration and its session pointer — and their
//! removal when the turn ends (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! A file under the agent's working directory (Cursor's `.cursor/mcp.json`, the
//! [`crate::session_pointer::SESSION_POINTER_FILE`]) is git-excluded through the repository's
//! `info/exclude` before it is written, so it can never land in a commit; a working directory that
//! is not a git checkout is refused, since nothing would keep the file out of one.

use std::path::{Path, PathBuf};

use crate::error::BackendError;
use crate::session_pointer::{SessionPointer, SessionPointerHeld, SESSION_POINTER_FILE};

/// The mode a turn file is written with: an MCP registration can carry the agent's tool credential.
const OWNER_ONLY_FILE: u32 = 0o600;

/// The files a turn wrote. Dropping it removes every one of them, however the turn ended.
#[derive(Debug)]
pub struct TurnFiles {
    fs: std::sync::Arc<dyn tddy_fs::Fs>,
    written: Vec<PathBuf>,
    /// Directories under the worktree this turn created, removed (when empty) after its files.
    created_dirs: Vec<PathBuf>,
}

impl Default for TurnFiles {
    fn default() -> Self {
        Self {
            fs: std::sync::Arc::new(tddy_fs::DiskFs),
            written: Vec::new(),
            created_dirs: Vec::new(),
        }
    }
}

impl TurnFiles {
    /// Writes `files` owner-only (creating their directories), git-excluding each that lies under
    /// `workdir`, and returns the guard that removes them.
    pub fn write(files: &[(PathBuf, Vec<u8>)], workdir: &Path) -> Result<Self, BackendError> {
        Self::write_on(std::sync::Arc::new(tddy_fs::DiskFs), files, workdir)
    }

    /// [`TurnFiles::write`] through the filesystem port.
    pub fn write_on(
        fs: std::sync::Arc<dyn tddy_fs::Fs>,
        files: &[(PathBuf, Vec<u8>)],
        workdir: &Path,
    ) -> Result<Self, BackendError> {
        let mut turn_files = Self {
            fs,
            written: Vec::new(),
            created_dirs: Vec::new(),
        };
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
        // The pointer is this backend's own file: a stale one a crashed turn left, or this
        // session's from its previous turn, is replaced. One a live owner holds for another
        // session is not.
        if let Some(held) = held_pointer(&*self.fs, &path, pointer) {
            return Err(BackendError::InvocationFailed(held.to_string()));
        }
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
        // `metadata`, as before: the question stays about the name itself.
        if path.starts_with(workdir) && self.fs.metadata(path).is_ok() {
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
        if let Some(parent) = path.parent().filter(|parent| {
            path.starts_with(workdir) && *parent != workdir && !self.fs.exists(parent)
        }) {
            self.created_dirs.push(parent.to_path_buf());
        }
        if let Ok(relative) = path.strip_prefix(workdir) {
            // Excluded before it is written, so it is never an untracked file git would offer.
            exclude_from_git_on(
                &*self.fs,
                workdir,
                &format!("/{}", relative.to_string_lossy()),
            )?;
        }
        self.fs
            .write_atomic(path, contents, Some(OWNER_ONLY_FILE))
            .map_err(|e| {
                BackendError::InvocationFailed(format!("could not write {}: {e}", path.display()))
            })?;
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
            if let Err(e) = self.fs.remove_file(path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("[turn-files] could not remove {}: {e}", path.display());
                }
            }
        }
        for dir in &self.created_dirs {
            // Fails, harmlessly, when something else has been put in it since.
            let _ = self.fs.remove_dir(dir);
        }
    }
}

/// The pointer at `path` when it is held for a session other than `incoming`'s: it names another
/// socket, and its owner is running on this host. A file that is missing, unreadable, ownerless,
/// owned elsewhere or by a process that has exited is not held.
fn held_pointer(
    fs: &dyn tddy_fs::Fs,
    path: &Path,
    incoming: &SessionPointer,
) -> Option<SessionPointerHeld> {
    let existing: SessionPointer = serde_json::from_slice(&fs.read(path).ok()?).ok()?;
    let owner = existing
        .owner
        .clone()
        .filter(|owner| owner.is_on_this_host() && owner.is_running())?;
    (existing.socket != incoming.socket).then(|| SessionPointerHeld {
        path: path.to_path_buf(),
        socket: existing.socket,
        owner,
    })
}

/// Adds `pattern` (`/.cursor/mcp.json`) to the `info/exclude` of the repository `worktree` is a
/// checkout of, unless it is there already. Refused when `worktree` is not a git checkout.
pub fn exclude_from_git(worktree: &Path, pattern: &str) -> Result<(), BackendError> {
    exclude_from_git_on(&tddy_fs::DiskFs, worktree, pattern)
}

/// [`exclude_from_git`] through the filesystem port.
pub fn exclude_from_git_on(
    fs: &dyn tddy_fs::Fs,
    worktree: &Path,
    pattern: &str,
) -> Result<(), BackendError> {
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
    let existing = match fs.read_to_string(&exclude) {
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
    fs.create_dir_all(&common_dir.join("info"))
        .map_err(io_error)?;
    fs.append(&exclude, format!("{separator}{pattern}\n").as_bytes())
        .map_err(io_error)
}
