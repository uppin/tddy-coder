//! The in-memory adapter, for tests.

use std::io;
use std::path::{Path, PathBuf};

use crate::{Fs, FsMetadata};

/// [`Fs`] over an in-memory tree: files, directories and modes, with the same atomic-write
/// guarantees as [`crate::DiskFs`], and fault injection for the failure paths a real disk only
/// shows when it is full.
#[derive(Debug, Default)]
pub struct MemFs {
    // TODO(fs-crate): implement — the tree and the armed fault.
}

impl MemFs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the next write that reaches the disk fail with `kind` part-way through — after some
    /// bytes of the new contents are written, as `ENOSPC` does.
    pub fn fail_next_write(&self, _kind: io::ErrorKind) {
        // TODO(fs-crate): implement
    }

    /// Every path the tree holds, files and directories, sorted.
    #[must_use]
    pub fn paths(&self) -> Vec<PathBuf> {
        // TODO(fs-crate): implement
        Vec::new()
    }
}

fn todo() -> io::Error {
    io::Error::other("TODO(fs-crate): implement")
}

impl Fs for MemFs {
    fn read(&self, _path: &Path) -> io::Result<Vec<u8>> {
        Err(todo())
    }
    fn read_to_string(&self, _path: &Path) -> io::Result<String> {
        Err(todo())
    }
    fn write(&self, _path: &Path, _contents: &[u8]) -> io::Result<()> {
        Err(todo())
    }
    fn append(&self, _path: &Path, _contents: &[u8]) -> io::Result<()> {
        Err(todo())
    }
    fn write_atomic(&self, _path: &Path, _contents: &[u8], _mode: Option<u32>) -> io::Result<()> {
        Err(todo())
    }
    fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
        Err(todo())
    }
    fn remove_file(&self, _path: &Path) -> io::Result<()> {
        Err(todo())
    }
    fn remove_dir_all(&self, _path: &Path) -> io::Result<()> {
        Err(todo())
    }
    fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(todo())
    }
    fn metadata(&self, _path: &Path) -> io::Result<FsMetadata> {
        Err(todo())
    }
    fn read_dir(&self, _path: &Path) -> io::Result<Vec<PathBuf>> {
        Err(todo())
    }
    fn set_mode(&self, _path: &Path, _mode: u32) -> io::Result<()> {
        Err(todo())
    }
    fn canonicalize(&self, _path: &Path) -> io::Result<PathBuf> {
        Err(todo())
    }
}
