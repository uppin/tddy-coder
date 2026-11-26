//! The real-disk adapter.
//!
//! [`DiskFs`] is [`Fs`] over the process's filesystem, including the crash-safe atomic write —
//! moved here from `tddy-session-store/src/atomic_file.rs`, which re-exports it at its old path.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{Fs, FsKind, FsMetadata};

/// [`Fs`] over the process's real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiskFs;

impl Fs for DiskFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        fs::write(path, contents)
    }

    fn append(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(contents)
    }

    fn write_atomic(&self, path: &Path, contents: &[u8], mode: Option<u32>) -> io::Result<()> {
        write_swapped(path, contents, mode)
    }

    fn create_dir_all_with_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        create_dir_with_mode(path, mode)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn hard_link(&self, original: &Path, link: &Path) -> io::Result<()> {
        fs::hard_link(original, link)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn metadata_follow(&self, path: &Path) -> io::Result<FsMetadata> {
        // `metadata`, not `symlink_metadata`: a final symlink resolves to its target.
        Ok(describe(&fs::metadata(path)?))
    }

    fn metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        Ok(describe(&fs::symlink_metadata(path)?))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<_>>()?;
        entries.sort();
        Ok(entries)
    }

    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        set_mode(path, mode)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }
}

fn describe(meta: &fs::Metadata) -> FsMetadata {
    FsMetadata {
        kind: kind_of(meta),
        len: meta.len(),
        mode: file_mode(meta),
        modified: meta.modified().ok(),
        uid: owner_of(meta),
        dev: device_of(meta),
        ino: inode_of(meta),
    }
}

#[cfg(unix)]
fn kind_of(meta: &fs::Metadata) -> FsKind {
    use std::os::unix::fs::FileTypeExt;

    let file_type = meta.file_type();
    if file_type.is_symlink() {
        FsKind::Symlink
    } else if file_type.is_socket() {
        FsKind::Socket
    } else if meta.is_dir() {
        FsKind::Dir
    } else {
        FsKind::File
    }
}

#[cfg(not(unix))]
fn kind_of(meta: &fs::Metadata) -> FsKind {
    if meta.is_symlink() {
        FsKind::Symlink
    } else if meta.is_dir() {
        FsKind::Dir
    } else {
        FsKind::File
    }
}

#[cfg(unix)]
fn owner_of(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;

    Some(meta.uid())
}

#[cfg(not(unix))]
fn owner_of(_meta: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn device_of(meta: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;

    Some(meta.dev())
}

#[cfg(not(unix))]
fn device_of(_meta: &fs::Metadata) -> Option<u64> {
    None
}

#[cfg(unix)]
fn inode_of(meta: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;

    Some(meta.ino())
}

#[cfg(not(unix))]
fn inode_of(_meta: &fs::Metadata) -> Option<u64> {
    None
}

/// Create `path` and every missing parent at exactly `mode`.
#[cfg(unix)]
fn create_dir_with_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    // The mode rides on the creation itself, so no window at the process umask. An
    // existing directory keeps the mode its operator gave it — narrowing one the daemon did
    // not create is the caller's to decide, never this call's.
    fs::DirBuilder::new()
        .recursive(true)
        .mode(mode)
        .create(path)
}

/// Without Unix permission bits there is no mode to create with.
#[cfg(not(unix))]
fn create_dir_with_mode(path: &Path, _mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn file_mode(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;

    // Permission bits only — `st_mode` also carries the file type, which `FsMetadata::kind`
    // already reports.
    Some(meta.mode() & 0o7777)
}

#[cfg(not(unix))]
fn file_mode(_meta: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

/// Without Unix permission bits there is nothing a mode could set.
#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

// ---------------------------------------------------------------------------
// The atomic write: swap file next to the target, flush, rename over it.
// ---------------------------------------------------------------------------
//
// `std::fs::write` opens the target with `O_TRUNC`: the old contents are gone the moment the
// call starts, and every byte after that is only best-effort. When the filesystem is full the
// write returns `ENOSPC` half way through — or at `close`, after a partial writeback — and what
// survives on disk is a **truncated or empty** file where session state used to be.
//
// `write_swapped` writes a swap file next to the target instead, flushes it to disk, and only
// then `rename`s it over the target. `rename` within a directory is atomic, so a reader sees
// either the whole old file or the whole new one, never a half-written one. Anything that can
// fail — the allocation, the write, the `fsync` — fails while the swap file is the only thing at
// risk, and the target is left exactly as it was.

fn write_swapped(path: &Path, contents: &[u8], mode: Option<u32>) -> io::Result<()> {
    let dir = parent_dir(path);
    fs::create_dir_all(&dir)?;
    let swap = swap_path(path, &dir);
    // Created before the cleanup arm is armed, because the cleanup deletes the swap file: a path
    // this call did **not** create is not this call's to remove, and `create_swap` now refuses one
    // that already exists rather than opening it.
    let file = create_swap(&swap, mode)?;
    match write_swap_then_rename(file, &swap, path, &dir, contents, mode) {
        Ok(()) => Ok(()),
        Err(e) => {
            // The swap file is this call's private scratch space; a failed call must not leave it
            // behind for a directory listing (or a later reader) to trip over.
            let _ = fs::remove_file(&swap);
            Err(e)
        }
    }
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// A swap name unique to this call, so two processes (or two threads) replacing the same file
/// never write into each other's scratch file. A shared, fixed `.tmp` name would let the second
/// writer's `rename` publish the first writer's half-written bytes.
fn swap_path(path: &Path, dir: &Path) -> PathBuf {
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    dir.join(format!(
        ".{}.{}.{}.swap",
        base,
        std::process::id(),
        uuid::Uuid::now_v7()
    ))
}

fn write_swap_then_rename(
    mut file: File,
    swap: &Path,
    final_path: &Path,
    dir: &Path,
    contents: &[u8],
    mode: Option<u32>,
) -> io::Result<()> {
    file.write_all(contents)?;
    // Without this the bytes may still be in page cache, and a full disk reports `ENOSPC`
    // long after `write` returned success. Forcing it here keeps the failure on the swap
    // file, before the rename makes it the session's state.
    file.sync_all()?;
    // Closed before the rename, as it was when this function opened the file itself.
    drop(file);
    // A caller-stated mode is already on the swap file, set as it was created; carrying the
    // target's bits over it would undo the very thing that mode was asked for.
    if mode.is_none() {
        carry_over_permissions(final_path, swap)?;
    }

    // Windows `rename` refuses an existing target; POSIX replaces it atomically.
    #[cfg(windows)]
    {
        let _ = fs::remove_file(final_path);
    }
    fs::rename(swap, final_path)?;

    // Best-effort: persists the rename itself, so a power loss cannot leave the directory entry
    // pointing at neither file. A directory that cannot be opened for this is not a write failure.
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
    Ok(())
}

/// Create the swap file, at `mode` when the caller stated one.
///
/// The mode is passed to `open` rather than set afterwards: a `create` followed by
/// `set_permissions` leaves a window in which the file exists at the process umask, which for a
/// secret is a window in which anyone can read it.
///
/// `create_new`, not `create`: `OpenOptions::mode()` applies **only** to a file this call creates,
/// so a swap path that already exists would be opened at whatever mode it already carries and the
/// requested `0o600` would be silently ignored. [`swap_path`] makes a collision vanishingly
/// unlikely, but "unlikely" is not the guarantee a private key is owed — refusing the path makes
/// the owner-only mode structural rather than probabilistic.
#[cfg(unix)]
fn create_swap(swap: &Path, mode: Option<u32>) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    if let Some(mode) = mode {
        options.mode(mode);
    }
    options.open(swap)
}

#[cfg(not(unix))]
fn create_swap(swap: &Path, _mode: Option<u32>) -> io::Result<File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(swap)
}

#[cfg(unix)]
fn carry_over_permissions(final_path: &Path, swap: &Path) -> io::Result<()> {
    if let Ok(meta) = fs::metadata(final_path) {
        fs::set_permissions(swap, meta.permissions())?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn carry_over_permissions(_final_path: &Path, _swap: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{write_atomic, write_atomic_with_mode};

    fn swap_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".swap"))
            .collect()
    }

    /// A fresh path gets the whole content, and the swap file it went through is gone.
    #[test]
    fn writes_new_file_and_leaves_no_swap_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.yaml");

        write_atomic(&path, "session_id: abc\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "session_id: abc\n");
        assert!(
            swap_files(dir.path()).is_empty(),
            "swap file must not survive a successful write: {:?}",
            swap_files(dir.path())
        );
    }

    /// Replacing shorter content over longer leaves no tail of the old file behind.
    #[test]
    fn replaces_existing_file_without_leftover_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("changeset.yaml");
        write_atomic(&path, "a".repeat(500)).unwrap();

        write_atomic(&path, "short\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "short\n");
    }

    /// Missing parents are created rather than reported as a write failure.
    #[test]
    fn creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions").join("s1").join("job.json");

        write_atomic(&path, "{}").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    /// **The disk-full case.** When the write cannot complete, the file that was already on disk
    /// must still be the file that was already on disk — not an empty or truncated one.
    ///
    /// A read-only directory stands in for a full filesystem: both make the swap file impossible
    /// to create, which is precisely the point of writing the swap file first.
    #[cfg(unix)]
    #[test]
    fn failed_write_leaves_previous_contents_intact() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.yaml");
        write_atomic(&path, "session_id: still-here\n").unwrap();

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores the permission bits, so there is nothing for this case to observe there.
        let unwritable = File::create(dir.path().join(".probe")).is_err();

        let result = write_atomic(&path, "session_id: replacement\n");
        let after = fs::read_to_string(&path).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        if !unwritable {
            return;
        }
        assert!(
            result.is_err(),
            "write into a read-only directory must fail"
        );
        assert_eq!(
            after, "session_id: still-here\n",
            "a failed write must not truncate or empty the previous file"
        );
    }

    /// Two writers replacing the same file concurrently must each work on their own swap file, so
    /// the winner publishes a complete document rather than a mix of both.
    #[test]
    fn concurrent_writers_never_publish_a_mixed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.yaml");
        let bodies: Vec<String> = (0..8)
            .map(|i| format!("writer: {i}\n").repeat(400))
            .collect();

        std::thread::scope(|scope| {
            for body in &bodies {
                let path = path.clone();
                scope.spawn(move || write_atomic(&path, body.as_bytes()).unwrap());
            }
        });

        let published = fs::read_to_string(&path).unwrap();
        assert!(
            bodies.contains(&published),
            "published file must be exactly one writer's content, not a blend"
        );
        assert!(
            swap_files(dir.path()).is_empty(),
            "no swap files may survive concurrent writes"
        );
    }

    /// A secret's **first** write must land owner-only: `write_atomic` can only copy bits from a
    /// target that already exists, and a freshly generated private key has no such target.
    #[cfg(unix)]
    #[test]
    fn creates_a_new_file_at_the_requested_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host-prompt-key.pem");

        write_atomic_with_mode(&path, "-----BEGIN PRIVATE KEY-----\n", 0o600).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "a first write of a secret must not land at the process umask"
        );
    }

    /// A restrictive mode on the old file (`.session.yaml` holds a hook token) survives the
    /// replacement instead of widening to the process umask.
    #[cfg(unix)]
    #[test]
    fn keeps_the_existing_files_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".session.yaml");
        write_atomic(&path, "hook_token: secret\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&path, "hook_token: rotated\n").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "replacement must not widen the file's mode");
    }

    /// The owner-only mode is honoured **only when the file is created** — `OpenOptions::mode()` is
    /// ignored for a path that already exists. A swap file left behind by another process (or
    /// planted there) would therefore receive a secret at whatever mode that file already carries,
    /// so the write must refuse the path rather than reuse it.
    #[cfg(unix)]
    #[test]
    fn refuses_a_swap_path_that_already_exists_rather_than_writing_a_secret_into_it() {
        use std::os::unix::fs::PermissionsExt;

        // Given a swap file already sitting at the path this write would use, world-readable
        let dir = tempfile::tempdir().unwrap();
        let squatted = dir.path().join(".host-prompt-key.pem.squatted.swap");
        fs::write(&squatted, "planted\n").unwrap();
        fs::set_permissions(&squatted, fs::Permissions::from_mode(0o644)).unwrap();

        // When a secret is written through it at owner-only
        let created = create_swap(&squatted, Some(0o600));

        // Then
        assert_eq!(
            created.err().map(|e| e.kind()),
            Some(io::ErrorKind::AlreadyExists),
            "a swap path that already exists must be refused: the requested mode would be ignored, \
             and the secret would land at the mode the existing file already carries"
        );
        assert_eq!(
            fs::read_to_string(&squatted).unwrap(),
            "planted\n",
            "the refused write must not have truncated what was there"
        );
    }
}
