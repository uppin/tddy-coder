//! The files one headless turn writes — its MCP registration and its session pointer — and their
//! removal when the turn ends (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`.
//!
//! A file under the agent's working directory (Cursor's `.cursor/mcp.json`, the
//! [`crate::session_pointer::SESSION_POINTER_FILE`]) is git-excluded through the repository's
//! `info/exclude` before it is written, so it can never land in a commit; a working directory that
//! is not a git checkout is refused, since nothing would keep the file out of one.

use std::path::{Path, PathBuf};

use crate::error::BackendError;
use crate::session_pointer::SessionPointer;

/// The files a turn wrote. Dropping it removes every one of them, however the turn ended.
#[derive(Debug, Default)]
pub struct TurnFiles {
    written: Vec<PathBuf>,
}

impl TurnFiles {
    /// Writes `files` owner-only (creating their directories), git-excluding each that lies under
    /// `workdir`, and returns the guard that removes them.
    pub fn write(files: &[(PathBuf, Vec<u8>)], workdir: &Path) -> Result<Self, BackendError> {
        // TODO(headless-driver): implement.
        let _ = (files, workdir);
        todo!("headless-driver: TurnFiles::write")
    }

    /// Also writes `pointer` as `<workdir>/`[`crate::session_pointer::SESSION_POINTER_FILE`],
    /// git-excluded, removed with the rest.
    pub fn write_session_pointer(
        &mut self,
        workdir: &Path,
        pointer: &SessionPointer,
    ) -> Result<(), BackendError> {
        // TODO(headless-driver): implement.
        let _ = (workdir, pointer);
        todo!("headless-driver: TurnFiles::write_session_pointer")
    }

    /// The files written, in order.
    pub fn paths(&self) -> &[PathBuf] {
        &self.written
    }
}

impl Drop for TurnFiles {
    fn drop(&mut self) {
        // TODO(headless-driver): implement — remove each of `self.written`, logging a failure.
    }
}

/// Adds `pattern` (`/.cursor/mcp.json`) to the `info/exclude` of the repository `worktree` is a
/// checkout of, unless it is there already. Refused when `worktree` is not a git checkout.
///
/// TODO(headless-driver): the launcher's `exclude_from_git` (`tddy-session-lifecycle`
/// `coding_agent_launcher.rs`) is the same code; it moves onto this one when the launcher's
/// `write_cursor_project_mcp_config` is deleted.
pub fn exclude_from_git(worktree: &Path, pattern: &str) -> Result<(), BackendError> {
    // TODO(headless-driver): implement.
    let _ = (worktree, pattern);
    todo!("headless-driver: exclude_from_git")
}
