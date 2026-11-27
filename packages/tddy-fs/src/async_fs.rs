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

fn join_failed(join: tokio::task::JoinError) -> io::Error {
    io::Error::other(join)
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

    pub async fn read(&self, path: impl Into<PathBuf>) -> io::Result<Vec<u8>> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.read(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn read_to_string(&self, path: impl Into<PathBuf>) -> io::Result<String> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.read_to_string(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn write(&self, path: impl Into<PathBuf>, contents: Vec<u8>) -> io::Result<()> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.write(&path, &contents))
            .await
            .map_err(join_failed)?
    }

    pub async fn write_atomic(
        &self,
        path: impl Into<PathBuf>,
        contents: Vec<u8>,
        mode: Option<u32>,
    ) -> io::Result<()> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.write_atomic(&path, &contents, mode))
            .await
            .map_err(join_failed)?
    }

    pub async fn create_dir_all(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.create_dir_all(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn remove_file(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.remove_file(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn remove_dir_all(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.remove_dir_all(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn rename(&self, from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> io::Result<()> {
        let fs = self.fs.clone();
        let from = from.into();
        let to = to.into();
        tokio::task::spawn_blocking(move || fs.rename(&from, &to))
            .await
            .map_err(join_failed)?
    }

    pub async fn copy(&self, from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> io::Result<()> {
        let fs = self.fs.clone();
        let from = from.into();
        let to = to.into();
        tokio::task::spawn_blocking(move || fs.copy(&from, &to))
            .await
            .map_err(join_failed)?
    }

    pub async fn metadata(&self, path: impl Into<PathBuf>) -> io::Result<FsMetadata> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.metadata(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn read_dir(&self, path: impl Into<PathBuf>) -> io::Result<Vec<PathBuf>> {
        let fs = self.fs.clone();
        let path = path.into();
        tokio::task::spawn_blocking(move || fs.read_dir(&path))
            .await
            .map_err(join_failed)?
    }

    pub async fn exists(&self, path: impl AsRef<Path>) -> bool {
        self.fs.exists(path.as_ref())
    }
}
