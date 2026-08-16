//! Replace a file's contents without ever truncating the file that is already there.
//!
//! `std::fs::write` opens the target with `O_TRUNC`: the old contents are gone the moment the
//! call starts, and every byte after that is only best-effort. When the filesystem is full the
//! write returns `ENOSPC` half way through — or at `close`, after a partial writeback — and what
//! survives on disk is a **truncated or empty** file where session state used to be. That is how a
//! full disk turns a live session into a dead one: a 0-byte `.session.yaml` reads as "no session"
//! even though the agent process is fine.
//!
//! [`write_atomic`] writes a swap file next to the target instead, flushes it to disk, and only
//! then `rename`s it over the target. `rename` within a directory is atomic, so a reader sees
//! either the whole old file or the whole new one, never a half-written one. Anything that can
//! fail — the allocation, the write, the `fsync` — fails while the swap file is the only thing at
//! risk, and the target is left exactly as it was.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Atomically replace `path` with `contents`.
///
/// On success `path` holds all of `contents`. On failure `path` is untouched — it keeps its
/// previous contents, or stays absent if it never existed — and the swap file is cleaned up.
///
/// Missing parent directories are created. An existing target's permission bits are carried over
/// to the replacement, so a mode-0600 file (`.session.yaml` carries a hook token) does not widen
/// to the process umask when it is rewritten.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let dir = parent_dir(path);
    fs::create_dir_all(&dir)?;
    let swap = swap_path(path, &dir);
    match write_swap_then_rename(&swap, path, &dir, contents.as_ref()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // The swap file is this call's private scratch space; a failed call must not leave it
            // behind for a directory listing (or a later reader) to trip over.
            let _ = fs::remove_file(&swap);
            Err(e)
        }
    }
}

/// [`write_atomic`] with the target path folded into the error message.
///
/// Bare `io::Error`s from a swap-then-rename say `"No space left on device"` and nothing about
/// which file was being replaced, which is exactly the fact an operator needs from the log.
pub fn write_atomic_labelled(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    write_atomic(path, contents).map_err(|e| format!("{}: {e}", path.display()))
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
    swap: &Path,
    final_path: &Path,
    dir: &Path,
    contents: &[u8],
) -> io::Result<()> {
    {
        let mut file = File::create(swap)?;
        file.write_all(contents)?;
        // Without this the bytes may still be in page cache, and a full disk reports `ENOSPC`
        // long after `write` returned success. Forcing it here keeps the failure on the swap
        // file, before the rename makes it the session's state.
        file.sync_all()?;
    }
    carry_over_permissions(final_path, swap)?;

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
}
