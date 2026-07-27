//! Pre-session staging upload for start-session attachments.
//!
//! Bytes uploaded ahead of `StartSession` land under
//! `{sessions_base}/staged-attachments/{staging_id}/{file_name}`. The shape mirrors
//! [`crate::session_file_upload`] (`upload_id` → `staging_id`, no `session_id`).

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir, validate_segment};

/// Rejection message for an unsafe `staging_id` or `file_name`.
pub(crate) const UNSAFE_STAGED_SEGMENT_ERR: &str =
    "staging_id and file_name must each be a basename";

/// The directory one Start-Session form's staged files land in:
/// `{sessions_base}/staged-attachments/{staging_id}`.
#[must_use]
pub fn staging_dir_for(sessions_base: &Path, staging_id: &str) -> PathBuf {
    sessions_base
        .join("staged-attachments")
        .join(staging_id)
}

/// Appends one ordered chunk of a staged attachment. Returns `None` for a non-final chunk, and
/// `Some(absolute_path)` on the final (`last`) chunk.
pub fn write_staged_chunk(
    sessions_base: &Path,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
    last: bool,
) -> Result<Option<PathBuf>, Status> {
    let safe_staging = validate_segment(staging_id).map_err(|_| {
        Status::invalid_argument(UNSAFE_STAGED_SEGMENT_ERR)
    })?;
    let safe_name = validate_segment(file_name).map_err(|_| {
        Status::invalid_argument(UNSAFE_STAGED_SEGMENT_ERR)
    })?;

    let staging_root = sessions_base.join("staged-attachments");
    let dir = staging_root.join(safe_staging);
    std::fs::create_dir_all(&dir).map_err(|e| {
        log::error!("write_staged_chunk: create_dir_all {:?} failed: {}", dir, e);
        Status::internal(format!("failed to create staging dir: {}", e))
    })?;

    let canonical_dir = contained_canonical_dir(&staging_root, &dir)?;

    let target = canonical_dir.join(safe_name);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .map_err(|e| {
            log::error!("write_staged_chunk: open {:?} failed: {}", target, e);
            Status::internal(format!("failed to open staging target: {}", e))
        })?;
    file.write_all(data).map_err(|e| {
        log::error!("write_staged_chunk: write {:?} failed: {}", target, e);
        Status::internal(format!("failed to write staging chunk: {}", e))
    })?;

    if last {
        log::info!(
            "write_staged_chunk: completed {:?} ({} final byte(s))",
            target,
            data.len()
        );
        Ok(Some(target))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{staging_dir_for, write_staged_chunk};

    const STAGING_ID: &str = "22222222-2222-7222-8222-222222222222";

    #[test]
    fn appends_chunks_in_order_and_returns_the_absolute_host_path_on_the_last_chunk() {
        let base = tempfile::tempdir().unwrap();

        let first = write_staged_chunk(base.path(), STAGING_ID, "brief.md", b"# He", false).unwrap();
        let last = write_staged_chunk(base.path(), STAGING_ID, "brief.md", b"llo", true).unwrap();

        assert_eq!(first, None);
        let host_path = last.expect("final chunk returns the absolute host path");
        assert_eq!(
            host_path,
            staging_dir_for(base.path(), STAGING_ID).join("brief.md")
        );
        assert!(host_path.is_absolute());
        assert_eq!(std::fs::read(&host_path).unwrap(), b"# Hello");
    }

    #[test]
    fn rejects_a_traversal_file_name_without_writing() {
        let base = tempfile::tempdir().unwrap();
        let result = write_staged_chunk(base.path(), STAGING_ID, "../escape.txt", b"x", true);
        assert!(result.is_err());
        assert!(!staging_dir_for(base.path(), STAGING_ID).exists());
    }
}
