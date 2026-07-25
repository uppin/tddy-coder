//! `ConnectionService.ListSessionUploads` / `DeleteSessionUpload` — the host side of the Session
//! Inspector Files tab (docs/ft/web/session-files-inspector.md).
//!
//! The upload flow ([`crate::session_file_upload`]) writes each dropped file to
//! `{session_dir}/uploads/{upload_id}/{file_name}`, one `upload_id` subfolder per drag gesture.
//! This module reads that tree back as a flat, newest-first list so uploaded files stay repeatedly
//! usable, and deletes a single file on demand.
//!
//! `upload_id` and `file_name` are untrusted client input in both directions, so delete reuses the
//! exact segment-validation + canonicalize-and-contain guard the writer applies — a delete must
//! never be a weaker gate than a write.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir, validate_segment};

/// One uploaded file surfaced to the Files tab. `host_path` is the absolute path on the host and
/// `uploaded_at_ms` is the file's modification time in unix milliseconds.
pub struct UploadEntry {
    pub upload_id: String,
    pub file_name: String,
    pub host_path: PathBuf,
    pub size_bytes: u64,
    pub uploaded_at_ms: i64,
}

/// The session's uploads root: `{session_dir}/uploads`. Holds no untrusted component, so it is the
/// trusted base every per-drop directory must stay within.
fn uploads_root(sessions_base: &Path, session_id: &str) -> PathBuf {
    unified_session_dir_path(sessions_base, session_id).join("uploads")
}

/// The file's modification time in unix milliseconds; `0` if the mtime predates the epoch or is
/// unreadable (neither is expected for a just-written upload).
fn mtime_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lists every uploaded file for the session as one flat list across all `upload_id` folders,
/// sorted newest-first by modification time. Walks `uploads/{upload_id}/` one level deep, emitting
/// one entry per regular file. A missing uploads root yields an empty list (a session that never
/// had an upload is a normal case), not an error.
pub fn list_uploads(sessions_base: &Path, session_id: &str) -> Result<Vec<UploadEntry>, Status> {
    let root = uploads_root(sessions_base, session_id);
    let root_entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            log::error!("list_uploads: read_dir {root:?} failed: {e}");
            return Err(Status::internal(format!("failed to read uploads dir: {e}")));
        }
    };

    let mut uploads = Vec::new();
    for upload_dir in root_entries {
        let upload_dir = upload_dir.map_err(|e| {
            log::error!("list_uploads: read_dir entry in {root:?} failed: {e}");
            Status::internal(format!("failed to read uploads dir: {e}"))
        })?;
        if !upload_dir.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let upload_id = upload_dir.file_name().to_string_lossy().into_owned();
        let upload_path = upload_dir.path();

        let files = fs::read_dir(&upload_path).map_err(|e| {
            log::error!("list_uploads: read_dir {upload_path:?} failed: {e}");
            Status::internal(format!("failed to read uploads dir: {e}"))
        })?;
        for file in files {
            let file = file.map_err(|e| {
                log::error!("list_uploads: read_dir entry in {upload_path:?} failed: {e}");
                Status::internal(format!("failed to read uploads dir: {e}"))
            })?;
            let metadata = match file.metadata() {
                Ok(m) if m.is_file() => m,
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("list_uploads: metadata {:?} failed: {e}", file.path());
                    continue;
                }
            };
            uploads.push(UploadEntry {
                upload_id: upload_id.clone(),
                file_name: file.file_name().to_string_lossy().into_owned(),
                host_path: file.path(),
                size_bytes: metadata.len(),
                uploaded_at_ms: mtime_ms(&metadata),
            });
        }
    }

    // Newest first: the tab presents the most recently uploaded file at the top.
    uploads.sort_by(|a, b| b.uploaded_at_ms.cmp(&a.uploaded_at_ms));
    Ok(uploads)
}

/// Deletes a single uploaded file, addressed by its `upload_id` + `file_name`. Both are untrusted
/// client input, so each is validated as a safe basename and the target is confirmed to resolve
/// inside the session's uploads root (the guard the writer shares). If the file's `upload_id`
/// folder is left empty it is pruned so the uploads root does not accumulate stale directories. A
/// file that does not exist yields [`Status::not_found`]; an unsafe segment yields
/// [`Status::invalid_argument`] and removes nothing.
pub fn delete_upload(
    sessions_base: &Path,
    session_id: &str,
    upload_id: &str,
    file_name: &str,
) -> Result<(), Status> {
    let safe_upload = validate_segment(upload_id)?;
    let safe_name = validate_segment(file_name)?;

    let root = uploads_root(sessions_base, session_id);
    let dir = root.join(safe_upload);
    if !dir.exists() {
        return Err(Status::not_found("uploaded file not found"));
    }

    // Defense-in-depth: even though the validated segments cannot traverse, confirm the per-drop
    // dir resolves inside the trusted uploads root before removing anything.
    let canonical_dir = contained_canonical_dir(&root, &dir)?;
    let target = canonical_dir.join(safe_name);
    if !target.is_file() {
        return Err(Status::not_found("uploaded file not found"));
    }

    fs::remove_file(&target).map_err(|e| {
        log::error!("delete_upload: remove_file {target:?} failed: {e}");
        Status::internal(format!("failed to delete upload: {e}"))
    })?;

    // Prune the now-empty drop folder so the uploads root does not accumulate stale directories.
    if fs::read_dir(&canonical_dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        if let Err(e) = fs::remove_dir(&canonical_dir) {
            log::warn!("delete_upload: prune empty dir {canonical_dir:?} failed: {e}");
        }
    }

    Ok(())
}
