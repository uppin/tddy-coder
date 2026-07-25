//! Terminal file drop upload (docs/ft/web/web-terminal.md § File drop upload).
//!
//! Files dropped on the web terminal are chunked client-side and appended, in order, to
//! `{session_dir}/uploads/{upload_id}/{file_name}`. Each drag gesture gets a fresh `upload_id`
//! (UUID) subfolder so original filenames are preserved and collisions between drops are
//! impossible. The final chunk returns the file's absolute host path so the web can type it into
//! the terminal.
//!
//! `upload_id` and `file_name` are both untrusted client input that become path segments, so each
//! is validated as a pure basename (path separators, `.`/`..`, and the empty string are rejected).
//! A canonicalize-and-contain guard (mirroring [`crate::worktree_files`] /
//! [`crate::session_workflow_files`]) then confirms the per-drop directory resolves inside the
//! session's trusted `uploads` root before any bytes are written.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;

/// Rejection message for an `upload_id` or `file_name` that is not a safe single path segment.
pub(crate) const UNSAFE_SEGMENT_ERR: &str = "upload_id and file_name must each be a basename";

/// The directory a drop's files land in: `{session_dir}/uploads/{upload_id}`.
#[must_use]
pub fn upload_dir_for(sessions_base: &Path, session_id: &str, upload_id: &str) -> PathBuf {
    unified_session_dir_path(sessions_base, session_id)
        .join("uploads")
        .join(upload_id)
}

/// Appends one ordered chunk of an uploaded file. Returns `None` for a non-final chunk, and
/// `Some(absolute_path)` on the final (`last`) chunk. Rejects an unsafe `upload_id` or `file_name`
/// with [`Status::invalid_argument`] and writes nothing in that case.
pub fn write_upload_chunk(
    sessions_base: &Path,
    session_id: &str,
    upload_id: &str,
    file_name: &str,
    data: &[u8],
    last: bool,
) -> Result<Option<PathBuf>, Status> {
    // Both untrusted segments must be plain basenames — neither may introduce a separator or `..`
    // that would climb out of the session's uploads directory.
    let safe_upload = validate_segment(upload_id)?;
    let safe_name = validate_segment(file_name)?;

    // The uploads root contains no untrusted component, so it is the trusted base the per-drop
    // directory must stay within.
    let uploads_root = unified_session_dir_path(sessions_base, session_id).join("uploads");
    let dir = uploads_root.join(safe_upload);
    std::fs::create_dir_all(&dir).map_err(|e| {
        log::error!("write_upload_chunk: create_dir_all {:?} failed: {}", dir, e);
        Status::internal(format!("failed to create uploads dir: {}", e))
    })?;

    // Canonicalize the trusted uploads root and the per-drop dir, and confirm the latter resolves
    // inside the former — defends against a symlink escape even though the validated segments
    // already cannot traverse. Rooting at `uploads_root` (not `dir`) makes this a real check rather
    // than a tautology.
    let canonical_dir = contained_canonical_dir(&uploads_root, &dir)?;

    let target = canonical_dir.join(safe_name);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .map_err(|e| {
            log::error!("write_upload_chunk: open {:?} failed: {}", target, e);
            Status::internal(format!("failed to open upload target: {}", e))
        })?;
    file.write_all(data).map_err(|e| {
        log::error!("write_upload_chunk: write {:?} failed: {}", target, e);
        Status::internal(format!("failed to write upload chunk: {}", e))
    })?;

    if last {
        log::info!(
            "write_upload_chunk: completed {:?} ({} final byte(s))",
            target,
            data.len()
        );
        Ok(Some(target))
    } else {
        Ok(None)
    }
}

/// Validates that `value` is a single safe path segment (a basename) and returns it. Rejects empty,
/// `.`, `..`, any value containing a path separator, and any value whose [`Path::file_name`]
/// differs from the input. Applied to both the `upload_id` and the `file_name` by every operation
/// (upload, list, delete) that turns untrusted client input into an `uploads` path segment.
pub(crate) fn validate_segment(value: &str) -> Result<&str, Status> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        log::warn!("session uploads: rejected unsafe path segment: {value:?}");
        return Err(Status::invalid_argument(UNSAFE_SEGMENT_ERR));
    }
    match Path::new(value).file_name() {
        Some(name) if name == value => Ok(value),
        _ => {
            log::warn!("session uploads: rejected non-basename path segment: {value:?}");
            Err(Status::invalid_argument(UNSAFE_SEGMENT_ERR))
        }
    }
}

/// Canonicalizes the trusted `uploads_root` and a candidate per-drop `dir` and confirms the latter
/// resolves inside the former, returning the canonical `dir`. Rooting the containment check at
/// `uploads_root` (which holds no untrusted component) rather than `dir` makes it a real guard
/// against a symlink escape, not a tautology. Shared by the upload writer and the delete path so a
/// delete is never a weaker gate than a write. Both paths must already exist on disk.
pub(crate) fn contained_canonical_dir(uploads_root: &Path, dir: &Path) -> Result<PathBuf, Status> {
    let canonical_root = uploads_root.canonicalize().map_err(|e| {
        log::error!("session uploads: canonicalize {uploads_root:?} failed: {e}");
        Status::internal(format!("failed to resolve uploads dir: {e}"))
    })?;
    let canonical_dir = dir.canonicalize().map_err(|e| {
        log::error!("session uploads: canonicalize {dir:?} failed: {e}");
        Status::internal(format!("failed to resolve uploads dir: {e}"))
    })?;
    if !canonical_dir.starts_with(&canonical_root) {
        log::warn!(
            "session uploads: rejected upload dir escaping uploads root: dir={canonical_dir:?} root={canonical_root:?}"
        );
        return Err(Status::invalid_argument(UNSAFE_SEGMENT_ERR));
    }
    Ok(canonical_dir)
}
