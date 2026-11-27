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

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir_on, validate_segment};

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

/// The file's modification time in unix milliseconds; `0` when the port reports no mtime, or
/// it predates the epoch (neither is expected for a just-written upload).
fn mtime_ms(metadata: &tddy_fs::FsMetadata) -> i64 {
    metadata
        .modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lists every uploaded file for the session as one flat list across all `upload_id` folders,
/// sorted newest-first by modification time. Walks `uploads/{upload_id}/` one level deep, emitting
/// one entry per regular file. A missing uploads root yields an empty list (a session that never
/// had an upload is a normal case), not an error.
pub fn list_uploads(sessions_base: &Path, session_id: &str) -> Result<Vec<UploadEntry>, Status> {
    list_uploads_on(&tddy_fs::DiskFs, sessions_base, session_id)
}

/// [`list_uploads`] through the filesystem port.
pub fn list_uploads_on(
    fs: &dyn tddy_fs::Fs,
    sessions_base: &Path,
    session_id: &str,
) -> Result<Vec<UploadEntry>, Status> {
    let root = uploads_root(sessions_base, session_id);
    let root_entries = match fs.read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            log::error!("list_uploads: read_dir {root:?} failed: {e}");
            return Err(Status::internal(format!("failed to read uploads dir: {e}")));
        }
    };

    let mut uploads = Vec::new();
    for upload_path in root_entries {
        // `metadata`, as before: the walk does not follow a link, and a link holds no uploads.
        let upload_meta = match fs.metadata(&upload_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if upload_meta.kind != tddy_fs::FsKind::Dir {
            continue;
        }
        let upload_id = upload_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let files = fs.read_dir(&upload_path).map_err(|e| {
            log::error!("list_uploads: read_dir {upload_path:?} failed: {e}");
            Status::internal(format!("failed to read uploads dir: {e}"))
        })?;
        for path in files {
            // `metadata_follow`, as before: the walk follows a link, and a link to nothing
            // holds nothing.
            let metadata = match fs.metadata_follow(&path) {
                Ok(m) if m.kind == tddy_fs::FsKind::File => m,
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("list_uploads: metadata {path:?} failed: {e}");
                    continue;
                }
            };
            uploads.push(UploadEntry {
                upload_id: upload_id.clone(),
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                host_path: path,
                size_bytes: metadata.len,
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
    delete_upload_on(
        &tddy_fs::DiskFs,
        sessions_base,
        session_id,
        upload_id,
        file_name,
    )
}

/// [`delete_upload`] through the filesystem port.
pub fn delete_upload_on(
    fs: &dyn tddy_fs::Fs,
    sessions_base: &Path,
    session_id: &str,
    upload_id: &str,
    file_name: &str,
) -> Result<(), Status> {
    let safe_upload = validate_segment(upload_id)?;
    let safe_name = validate_segment(file_name)?;

    let root = uploads_root(sessions_base, session_id);
    let dir = root.join(safe_upload);
    if !fs.exists(&dir) {
        return Err(Status::not_found("uploaded file not found"));
    }

    // Defense-in-depth: even though the validated segments cannot traverse, confirm the per-drop
    // dir resolves inside the trusted uploads root before removing anything.
    let canonical_dir = contained_canonical_dir_on(fs, &root, &dir)?;
    let target = canonical_dir.join(safe_name);
    match fs.metadata_follow(&target) {
        Ok(m) if m.kind == tddy_fs::FsKind::File => {}
        _ => return Err(Status::not_found("uploaded file not found")),
    }

    fs.remove_file(&target).map_err(|e| {
        log::error!("delete_upload: remove_file {target:?} failed: {e}");
        Status::internal(format!("failed to delete upload: {e}"))
    })?;

    // Prune the now-empty drop folder so the uploads root does not accumulate stale directories.
    if fs
        .read_dir(&canonical_dir)
        .map(|entries| entries.is_empty())
        .unwrap_or(false)
    {
        if let Err(e) = fs.remove_dir(&canonical_dir) {
            log::warn!("delete_upload: prune empty dir {canonical_dir:?} failed: {e}");
        }
    }

    Ok(())
}
