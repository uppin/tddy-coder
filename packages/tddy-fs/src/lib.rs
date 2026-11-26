//! The filesystem port.
//!
//! Every disk read and write in tddy goes through [`Fs`], so behaviour that depends on disk can be
//! driven against [`MemFs`] in a test and against [`DiskFs`] in production. The crash-safe
//! "replace a file without ever truncating it" write is a port operation ([`Fs::write_atomic`]),
//! so there is one implementation of it, not one per crate.

#[cfg(feature = "tokio")]
mod async_fs;
mod disk;
mod mem;

use std::fmt::Debug;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[cfg(feature = "tokio")]
pub use async_fs::AsyncFs;
pub use disk::DiskFs;
pub use mem::MemFs;

/// What [`Fs::metadata`] reports about a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsMetadata {
    pub kind: FsKind,
    pub len: u64,
    /// Unix permission bits; `None` on platforms without them.
    pub mode: Option<u32>,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    File,
    Dir,
    Symlink,
}

/// The filesystem port. Object safe: hold it as `Arc<dyn Fs>`.
pub trait Fs: Send + Sync + Debug {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Plain write (truncating). Prefer [`Fs::write_atomic`] for anything a crash must not lose.
    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn append(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    /// Replace `path` with `contents` so a reader sees the whole old or the whole new file.
    ///
    /// Missing parents are created. With `mode` the replacement is created at exactly that mode
    /// (never wider, not even briefly); without it an existing target's mode is carried over. On
    /// failure `path` is untouched and no swap file is left behind.
    fn write_atomic(&self, path: &Path, contents: &[u8], mode: Option<u32>) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn metadata(&self, path: &Path) -> io::Result<FsMetadata>;
    /// The entries of `path`, as full paths, sorted.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }
}

/// [`Fs::write_atomic`] on the real disk, keeping the target's mode — the former
/// `tddy_session_store::atomic_file::write_atomic`.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    DiskFs.write_atomic(path, contents.as_ref(), None)
}

/// [`Fs::write_atomic`] on the real disk at a fixed `mode`.
pub fn write_atomic_with_mode(
    path: &Path,
    contents: impl AsRef<[u8]>,
    mode: u32,
) -> io::Result<()> {
    DiskFs.write_atomic(path, contents.as_ref(), Some(mode))
}

/// [`write_atomic`] with the target path folded into the error message.
pub fn write_atomic_labelled(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    write_atomic(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}
