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
/// Support for tests of crates migrating onto the port.
#[cfg(feature = "testing")]
pub mod testing;

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
    /// Owning user id; `None` on platforms without one. The daemon's socket and `users.yaml`
    /// guards refuse files owned by anyone outside an allowed set, so ownership travels with
    /// the stat rather than through a second platform call.
    pub uid: Option<u32>,
    /// Device and inode: the identity a socket server compares to notice its file was replaced
    /// or removed. `None` on platforms without them.
    pub dev: Option<u64>,
    pub ino: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    File,
    Dir,
    Symlink,
    /// A unix-domain socket file: what the daemon binds, and what a stale-socket clear must
    /// check for before removing anything.
    Socket,
}

/// The filesystem port. Object safe: hold it as `Arc<dyn Fs>`.
pub trait Fs: Send + Sync + Debug {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Plain write (truncating). Prefer [`Fs::write_atomic`] for anything a crash must not lose.
    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    /// Create `path` holding `contents`, failing with [`io::ErrorKind::AlreadyExists`] when
    /// anything — a file, directory or symlink — already exists there: an exclusive create, so
    /// a concurrent creator cannot slip between the existence check and the creation. The loser
    /// leaves the stored bytes untouched. Parents are not created; create them first.
    fn create_new(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    fn append(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    /// Replace `path` with `contents` so a reader sees the whole old or the whole new file.
    ///
    /// Missing parents are created. With `mode` the replacement is created at exactly that mode
    /// (never wider, not even briefly); without it an existing target's mode is carried over. On
    /// failure `path` is untouched and no swap file is left behind.
    fn write_atomic(&self, path: &Path, contents: &[u8], mode: Option<u32>) -> io::Result<()>;
    /// Create `path` and every missing parent with exactly `mode`, never wider, not even
    /// briefly: the mode is given to the creation, not applied afterwards. A `create_dir_all`
    /// followed by `set_mode` leaves a window in which the directory is listable at the process
    /// umask — for the owner-only socket and auth-storage directories, a window in which anyone
    /// can list them.
    fn create_dir_all_with_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Make `link` a hard link to `original`, failing when `link` already exists: unlike
    /// `rename`, a link never silently replaces the winner of a first-boot race (two daemons
    /// generating a signing key against one data directory — the loser keeps the winner's key
    /// rather than publishing its own over it).
    fn hard_link(&self, original: &Path, link: &Path) -> io::Result<()>;
    /// Copy the file at `from` to `to`, creating `to` or truncating it. Fails when `from` is
    /// missing or is not a file. Parents are not created. On DiskFs this is `std::fs::copy`,
    /// so the copy carries the source's mode; MemFs mints the copy with the default file mode.
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Remove an empty directory, failing on a non-empty one: `remove_dir_all` would delete a
    /// directory the caller did not create (a failed bind's cleanup removing only what it made).
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Like [`Fs::metadata`], but following a final symlink: what `execve` does too, so a `PATH`
    /// lookup for an executable must see the target's file bits rather than the link's.
    fn metadata_follow(&self, path: &Path) -> io::Result<FsMetadata>;
    fn metadata(&self, path: &Path) -> io::Result<FsMetadata>;
    /// The entries of `path`, as full paths, sorted.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }
}

/// Atomically replace `path` with `contents` on the real disk — the former
/// `tddy_session_store::atomic_file::write_atomic`.
///
/// On success `path` holds all of `contents`. On failure `path` is untouched — it keeps its
/// previous contents, or stays absent if it never existed — and the swap file is cleaned up.
///
/// Missing parent directories are created. An existing target's permission bits are carried over
/// to the replacement, so a mode-0600 file does not widen to the process umask when it is
/// rewritten. A target that does **not** exist yet gets whatever the process umask allows — for a
/// secret, use [`write_atomic_with_mode`] instead.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    DiskFs.write_atomic(path, contents.as_ref(), None)
}

/// [`write_atomic`], with the replacement's permissions fixed by the caller instead of copied from
/// the target.
///
/// `write_atomic` can only carry over bits that already exist, so the **first** write to a path
/// creates the swap file at the process umask — under the usual `0o022` that is a world-readable
/// file. For session state that is merely untidy; for a secret it is a disclosure, and the first
/// write is exactly when a secret is created. `mode` is applied when the swap file is created, so
/// the bytes are never on disk at any wider mode, not even briefly.
///
/// `mode` is a Unix permission bitmask (`0o600` for owner-only). It is ignored on platforms without
/// Unix permissions, where this behaves as [`write_atomic`].
pub fn write_atomic_with_mode(
    path: &Path,
    contents: impl AsRef<[u8]>,
    mode: u32,
) -> io::Result<()> {
    DiskFs.write_atomic(path, contents.as_ref(), Some(mode))
}

/// [`write_atomic`] with the target path folded into the error message.
///
/// Bare `io::Error`s from a swap-then-rename say `"No space left on device"` and nothing about
/// which file was being replaced, which is exactly the fact an operator needs from the log.
pub fn write_atomic_labelled(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    write_atomic(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}
