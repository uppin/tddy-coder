//! Pre-session **staging area** for start-session attachments.
//!
//! Documents picked in the Start-Session form are uploaded ahead of the session (before a
//! session directory exists) into a per-host, per-caller staging root at
//! `{staging_base_dir}/{os_user}/{staging_id}/{file_name}`, then referenced from
//! `StartSessionRequest.attachments` via a `StagedAttachmentRef`. The base defaults to
//! [`default_staging_base_dir`] — under the process temp dir, so a host restart clears whatever
//! was abandoned rather than accumulating batches forever under the data dir. The materialization path
//! (see `connection_service::materialize_session_attachments`) copies a staged file into the
//! new session's `artifacts/attachments/` before the agent launches.
//!
//! `staging_id` and `file_name` are untrusted client input that become path segments, so each is
//! validated as a pure basename (the uploads path's `validate_segment` guard) and the per-batch
//! directory is canonicalize-and-contained under the caller's staging root — the same guard shape
//! as `session_file_upload`. The chunked append mirrors `write_upload_chunk` so `tddy-web`'s
//! `lib/fileUploadChunks.ts` stays reusable unchanged.
//!
//! Product contract: `docs/ft/coder/session-attachments.md` + the amendment
//! `docs/ft/coder/session-attachments.md` § Start-session materialization.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir_on, validate_segment};

/// Suffix marking a staged file as fully uploaded (no further chunks, no re-upload).
const STAGED_COMPLETE_SUFFIX: &str = ".staged-complete";

/// Directory name the staging base takes under the process temp dir.
const STAGING_BASE_DIR_NAME: &str = "tddy-staging";

/// The staging base a daemon uses unless one is injected: `{temp_dir}/tddy-staging/`. A staged
/// batch is only useful between its upload and the `StartSession` that consumes it, and nothing
/// deletes a consumed or abandoned batch — so the root lives where the host clears it on restart,
/// which bounds abandonment without a TTL, a GC job or any new failure mode.
#[must_use]
pub fn default_staging_base_dir() -> PathBuf {
    std::env::temp_dir().join(STAGING_BASE_DIR_NAME)
}

/// Per-host, per-caller staging root: `{staging_base_dir}/{os_user}/`. Holds no untrusted
/// component beyond `os_user` (which the daemon resolves from the session token), so it is the
/// trusted base every per-batch directory must stay within.
#[must_use]
pub fn staging_root_for(os_user: &str, staging_base_dir: &Path) -> PathBuf {
    staging_base_dir.join(os_user)
}

/// One file sitting in a host's pre-session staging area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedAttachmentFile {
    pub staging_id: String,
    pub file_name: String,
    pub host_path: PathBuf,
    pub size_bytes: u64,
    pub staged_at_ms: i64,
}

fn mtime_ms(metadata: &tddy_fs::FsMetadata) -> i64 {
    metadata
        .modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn staged_complete_marker(canonical_dir: &Path, file_name: &str) -> PathBuf {
    canonical_dir.join(format!("{file_name}{STAGED_COMPLETE_SUFFIX}"))
}

/// The staged-at timestamp of one file already on disk, in unix milliseconds, through the
/// filesystem port.
///
/// Shares [`mtime_ms`] with [`list_staged_attachments_on`] so an entry answered on the upload's
/// final chunk carries the same timestamp a later listing reports for it. Two derivations would be
/// two answers to "when was this staged", and the form would sort by one and display the other.
#[must_use]
pub fn staged_at_ms_on(fs: &dyn tddy_fs::Fs, host_path: &Path) -> i64 {
    fs.metadata_follow(host_path)
        .map(|m| mtime_ms(&m))
        .unwrap_or(0)
}

fn is_staged_complete_marker_name(name: &str) -> bool {
    name.ends_with(STAGED_COMPLETE_SUFFIX)
}

/// Appends one ordered chunk of a staged file. Returns `None` for a non-final chunk and
/// `Some(absolute_path)` on the final (`last`) chunk. Rejects an unsafe `staging_id` or
/// `file_name` with [`Status::invalid_argument`] and writes nothing in that case.
pub fn write_staged_chunk(
    staging_root: &Path,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
    last: bool,
) -> Result<Option<PathBuf>, Status> {
    write_staged_chunk_on(
        &tddy_fs::DiskFs,
        staging_root,
        staging_id,
        file_name,
        data,
        last,
    )
}

/// [`write_staged_chunk`] through the filesystem port.
pub fn write_staged_chunk_on(
    fs: &dyn tddy_fs::Fs,
    staging_root: &Path,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
    last: bool,
) -> Result<Option<PathBuf>, Status> {
    let safe_staging = validate_segment(staging_id)?;
    let safe_name = validate_segment(file_name)?;

    fs.create_dir_all(staging_root).map_err(|e| {
        log::error!(
            "write_staged_chunk: create_dir_all {:?} failed: {}",
            staging_root,
            e
        );
        Status::internal(format!("failed to create staging root: {}", e))
    })?;

    let dir = staging_root.join(safe_staging);
    fs.create_dir_all(&dir).map_err(|e| {
        log::error!("write_staged_chunk: create_dir_all {:?} failed: {}", dir, e);
        Status::internal(format!("failed to create staging batch dir: {}", e))
    })?;

    let canonical_dir = contained_canonical_dir_on(fs, staging_root, &dir)?;

    let complete_marker = staged_complete_marker(&canonical_dir, safe_name);
    if fs.exists(&complete_marker) {
        log::warn!(
            "write_staged_chunk: refusing to overwrite completed staged file {:?}",
            safe_name
        );
        return Err(Status::invalid_argument(
            "staged file already exists in this batch",
        ));
    }

    let target = canonical_dir.join(safe_name);
    fs.append(&target, data).map_err(|e| {
        log::error!("write_staged_chunk: append {:?} failed: {}", target, e);
        Status::internal(format!("failed to write staged chunk: {}", e))
    })?;

    if last {
        fs.write(&complete_marker, b"").map_err(|e| {
            log::error!(
                "write_staged_chunk: write complete marker {:?} failed: {}",
                complete_marker,
                e
            );
            Status::internal(format!("failed to finalize staged file: {}", e))
        })?;
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

fn collect_batch_files(
    fs: &dyn tddy_fs::Fs,
    staging_id: &str,
    batch_path: &Path,
    out: &mut Vec<StagedAttachmentFile>,
) -> Result<(), Status> {
    let files = fs.read_dir(batch_path).map_err(|e| {
        log::error!("list_staged_attachments: read_dir {batch_path:?} failed: {e}");
        Status::internal(format!("failed to read staging batch dir: {e}"))
    })?;
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_staged_complete_marker_name(&name) {
            continue;
        }
        // `metadata_follow`, as before: the walk follows a link, and a link to nothing holds
        // nothing.
        let metadata = match fs.metadata_follow(&path) {
            Ok(m) if m.kind == tddy_fs::FsKind::File => m,
            Ok(_) => continue,
            Err(e) => {
                log::warn!("list_staged_attachments: metadata {path:?} failed: {e}");
                continue;
            }
        };
        out.push(StagedAttachmentFile {
            staging_id: staging_id.to_string(),
            file_name: name,
            host_path: path,
            size_bytes: metadata.len,
            staged_at_ms: mtime_ms(&metadata),
        });
    }
    Ok(())
}

/// Lists staged files for a batch (or every batch for the caller when `staging_id` is empty),
/// newest-first by modification time. A missing staging root yields an empty list, not an error.
pub fn list_staged_attachments(
    staging_root: &Path,
    staging_id: &str,
) -> Result<Vec<StagedAttachmentFile>, Status> {
    list_staged_attachments_on(&tddy_fs::DiskFs, staging_root, staging_id)
}

/// [`list_staged_attachments`] through the filesystem port.
pub fn list_staged_attachments_on(
    fs: &dyn tddy_fs::Fs,
    staging_root: &Path,
    staging_id: &str,
) -> Result<Vec<StagedAttachmentFile>, Status> {
    let root_entries = match fs.read_dir(staging_root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            log::error!("list_staged_attachments: read_dir {staging_root:?} failed: {e}");
            return Err(Status::internal(format!(
                "failed to read staging root: {e}"
            )));
        }
    };

    let mut files = Vec::new();

    if !staging_id.is_empty() {
        let safe_staging = validate_segment(staging_id)?;
        let batch_path = staging_root.join(safe_staging);
        // `metadata_follow`, as before: `is_dir` follows a link.
        if matches!(
            fs.metadata_follow(&batch_path),
            Ok(m) if m.kind == tddy_fs::FsKind::Dir
        ) {
            let canonical_dir = contained_canonical_dir_on(fs, staging_root, &batch_path)?;
            collect_batch_files(fs, safe_staging, &canonical_dir, &mut files)?;
        }
    } else {
        for batch_path in root_entries {
            // `metadata`, as before: the walk does not follow a link, and a link holds no batch.
            if !matches!(
                fs.metadata(&batch_path),
                Ok(m) if m.kind == tddy_fs::FsKind::Dir
            ) {
                continue;
            }
            let batch_staging_id = batch_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let canonical_dir = match contained_canonical_dir_on(fs, staging_root, &batch_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            collect_batch_files(fs, &batch_staging_id, &canonical_dir, &mut files)?;
        }
    }

    files.sort_by(|a, b| b.staged_at_ms.cmp(&a.staged_at_ms));
    Ok(files)
}

/// Removes one staged file. A delete is never a weaker gate than a write — same `validate_segment`
/// + canonicalize-and-contain guards as the writer.
pub fn delete_staged_attachment(
    staging_root: &Path,
    staging_id: &str,
    file_name: &str,
) -> Result<(), Status> {
    delete_staged_attachment_on(&tddy_fs::DiskFs, staging_root, staging_id, file_name)
}

/// [`delete_staged_attachment`] through the filesystem port.
pub fn delete_staged_attachment_on(
    fs: &dyn tddy_fs::Fs,
    staging_root: &Path,
    staging_id: &str,
    file_name: &str,
) -> Result<(), Status> {
    let safe_staging = validate_segment(staging_id)?;
    let safe_name = validate_segment(file_name)?;

    let dir = staging_root.join(safe_staging);
    if !fs.exists(&dir) {
        return Err(Status::not_found("staged file not found"));
    }

    let canonical_dir = contained_canonical_dir_on(fs, staging_root, &dir)?;
    let target = canonical_dir.join(safe_name);
    // `metadata_follow`, as before: `is_file` follows a link.
    if !matches!(
        fs.metadata_follow(&target),
        Ok(m) if m.kind == tddy_fs::FsKind::File
    ) {
        return Err(Status::not_found("staged file not found"));
    }

    fs.remove_file(&target).map_err(|e| {
        log::error!("delete_staged_attachment: remove_file {target:?} failed: {e}");
        Status::internal(format!("failed to delete staged file: {e}"))
    })?;

    let marker = staged_complete_marker(&canonical_dir, safe_name);
    if fs.exists(&marker) {
        if let Err(e) = fs.remove_file(&marker) {
            log::warn!("delete_staged_attachment: remove marker {marker:?} failed: {e}");
        }
    }

    Ok(())
}

#[cfg(all(test, unix))]
#[path = "session_attachment_staging_tests.rs"]
mod session_attachment_staging_tests;
