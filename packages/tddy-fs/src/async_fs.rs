//! The facade `tokio::fs` call sites migrate onto.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{Fs, FsMetadata};

/// An [`Fs`] for async code: each operation runs the port on tokio's blocking pool, as
/// `tokio::fs` does with `std::fs`.
#[derive(Debug, Clone)]
pub struct AsyncFs {
    fs: Arc<dyn Fs>,
}

fn todo() -> io::Error {
    io::Error::other("TODO(fs-crate): implement")
}

impl AsyncFs {
    #[must_use]
    pub fn new(fs: Arc<dyn Fs>) -> Self {
        Self { fs }
    }

    /// The port underneath, for a call that is fine blocking.
    #[must_use]
    pub fn sync(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    pub async fn read(&self, _path: impl Into<PathBuf>) -> io::Result<Vec<u8>> {
        Err(todo())
    }
    pub async fn read_to_string(&self, _path: impl Into<PathBuf>) -> io::Result<String> {
        Err(todo())
    }
    pub async fn write(&self, _path: impl Into<PathBuf>, _contents: Vec<u8>) -> io::Result<()> {
        Err(todo())
    }
    pub async fn write_atomic(
        &self,
        _path: impl Into<PathBuf>,
        _contents: Vec<u8>,
        _mode: Option<u32>,
    ) -> io::Result<()> {
        Err(todo())
    }
    pub async fn create_dir_all(&self, _path: impl Into<PathBuf>) -> io::Result<()> {
        Err(todo())
    }
    pub async fn remove_file(&self, _path: impl Into<PathBuf>) -> io::Result<()> {
        Err(todo())
    }
    pub async fn remove_dir_all(&self, _path: impl Into<PathBuf>) -> io::Result<()> {
        Err(todo())
    }
    pub async fn rename(
        &self,
        _from: impl Into<PathBuf>,
        _to: impl Into<PathBuf>,
    ) -> io::Result<()> {
        Err(todo())
    }
    pub async fn metadata(&self, _path: impl Into<PathBuf>) -> io::Result<FsMetadata> {
        Err(todo())
    }
    pub async fn read_dir(&self, _path: impl Into<PathBuf>) -> io::Result<Vec<PathBuf>> {
        Err(todo())
    }
    pub async fn exists(&self, path: impl AsRef<Path>) -> bool {
        self.fs.exists(path.as_ref())
    }
}
