//! Replace a vault file without ever truncating the one that is already there, owner-only from
//! the first byte.
//!
//! The same swap-then-rename `tddy_session_store::atomic_file::write_atomic_with_mode` performs,
//! reduced to the one mode this crate writes. It is repeated here rather than depended on because
//! that crate brings tokio, jsonschema and the workflow, task and action crates with it, and this
//! crate is meant to be taken by `#keyring` 6/9 and 7/9 without them.
//!
//! TODO(keyring): two copies of this invariant is one too many; a leaf crate both depend on is the
//! fix — docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rand::RngCore;

/// Owner-only: the file is ciphertext, but a readable one is a copy for an offline attempt.
const OWNER_ONLY: u32 = 0o600;

/// Atomically replace `path` with `contents`, at mode `0o600` from the moment the bytes exist.
///
/// Returns `Ok` only once the new contents and the rename that published them are both on disk:
/// the directory is synced after the rename, and a failure to sync it is an error too.
///
/// On a failure before the rename `path` is untouched — it keeps its previous contents, or stays
/// absent — and the swap file is removed. A failure to sync the directory comes **after** the
/// rename: `path` already holds the new contents, but a crash could still bring the old ones back,
/// so the caller is told the write is not durable rather than that it is.
pub(crate) fn write_owner_only(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&dir)?;
    let swap = swap_path(path, &dir);
    // Created before the cleanup can run: a path this call did not create is not its to remove.
    let file = create_swap(&swap)?;
    write_then_rename(file, &swap, path, &dir, contents).inspect_err(|_| {
        let _ = fs::remove_file(&swap);
    })
}

/// A swap name unique to this call, so two writers never publish each other's half-written bytes.
fn swap_path(path: &Path, dir: &Path) -> PathBuf {
    let mut nonce = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let base = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vault".to_string());
    dir.join(format!(
        ".{base}.{}.{}.swap",
        std::process::id(),
        crate::kdf::to_hex(&nonce)
    ))
}

fn write_then_rename(
    mut file: File,
    swap: &Path,
    final_path: &Path,
    dir: &Path,
    contents: &[u8],
) -> io::Result<()> {
    file.write_all(contents)?;
    // Forces a full disk to fail here, on the swap file, rather than after the rename.
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    {
        let _ = fs::remove_file(final_path);
    }
    fs::rename(swap, final_path)?;
    sync_dir(dir)
}

/// Persist the rename itself: without it, a crash can leave the directory naming the old file.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Windows cannot open a directory as a file to sync it; the rename is as durable as it gets there.
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// `create_new` at the owner-only mode: `mode` applies only to a file `open` creates, so an
/// existing swap path would keep whatever mode it has. Refusing one makes `0o600` structural.
#[cfg(unix)]
fn create_swap(swap: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(swap)
}

#[cfg(not(unix))]
fn create_swap(swap: &Path) -> io::Result<File> {
    let _ = OWNER_ONLY;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(swap)
}
