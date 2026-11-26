//! The real-disk adapter.

use std::io;
use std::path::{Path, PathBuf};

use crate::{Fs, FsMetadata};

/// [`Fs`] over the process's real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiskFs;

fn todo() -> io::Error {
    io::Error::other("TODO(fs-crate): implement")
}

// TODO(fs-crate): implement — move `tddy-session-store/src/atomic_file.rs` here for `write_atomic`.
impl Fs for DiskFs {
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
