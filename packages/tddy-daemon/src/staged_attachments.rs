//! `ListStagedAttachments` / `DeleteStagedAttachment` — list and remove pre-session staged files.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir, validate_segment};

/// One staged file surfaced to the Start-Session form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedEntry {
    pub staging_id: String,
    pub file_name: String,
    pub host_path: PathBuf,
    pub size_bytes: u64,
    pub staged_at_ms: i64,
}

fn staging_root(sessions_base: &Path) -> PathBuf {
    sessions_base.join("staged-attachments")
}

fn mtime_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lists staged files for the caller. When `staging_id` is non-empty, only that batch is returned;
/// otherwise every batch under the staging root is walked. Newest-first by mtime.
pub fn list_staged(
    sessions_base: &Path,
    staging_id: &str,
) -> Result<Vec<StagedEntry>, Status> {
    let root = staging_root(sessions_base);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let staging_dirs: Vec<PathBuf> = if staging_id.trim().is_empty() {
        let entries = fs::read_dir(&root).map_err(|e| {
            log::error!("list_staged: read_dir {root:?} failed: {e}");
            Status::internal(format!("failed to read staging dir: {e}"))
        })?;
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect()
    } else {
        let safe = validate_segment(staging_id).map_err(|_| {
            Status::invalid_argument("staging_id must be a basename")
        })?;
        vec![root.join(safe)]
    };

    let mut staged = Vec::new();
    for dir in staging_dirs {
        if !dir.is_dir() {
            continue;
        }
        let batch_id = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let files = match fs::read_dir(&dir) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                log::error!("list_staged: read_dir {dir:?} failed: {e}");
                return Err(Status::internal(format!("failed to read staging dir: {e}")));
            }
        };

        for file in files {
            let file = file.map_err(|e| {
                log::error!("list_staged: read_dir entry in {dir:?} failed: {e}");
                Status::internal(format!("failed to read staging dir: {e}"))
            })?;
            let metadata = match file.metadata() {
                Ok(m) if m.is_file() => m,
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("list_staged: metadata {:?} failed: {e}", file.path());
                    continue;
                }
            };
            staged.push(StagedEntry {
                staging_id: batch_id.clone(),
                file_name: file.file_name().to_string_lossy().into_owned(),
                host_path: file.path(),
                size_bytes: metadata.len(),
                staged_at_ms: mtime_ms(&metadata),
            });
        }
    }

    staged.sort_by(|a, b| b.staged_at_ms.cmp(&a.staged_at_ms));
    Ok(staged)
}

/// Deletes one staged file addressed by `staging_id` + `file_name`.
pub fn delete_staged(
    sessions_base: &Path,
    staging_id: &str,
    file_name: &str,
) -> Result<(), Status> {
    let safe_staging = validate_segment(staging_id).map_err(|_| {
        Status::invalid_argument("staging_id must be a basename")
    })?;
    let safe_name = validate_segment(file_name).map_err(|_| {
        Status::invalid_argument("file_name must be a basename")
    })?;

    let root = staging_root(sessions_base);
    let dir = root.join(safe_staging);
    if !dir.exists() {
        return Err(Status::not_found("staged file not found"));
    }

    let canonical_dir = contained_canonical_dir(&root, &dir)?;
    let target = canonical_dir.join(safe_name);
    if !target.is_file() {
        return Err(Status::not_found("staged file not found"));
    }

    fs::remove_file(&target).map_err(|e| {
        log::error!("delete_staged: remove_file {target:?} failed: {e}");
        Status::internal(format!("failed to delete staged file: {e}"))
    })?;

    if fs::read_dir(&canonical_dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        if let Err(e) = fs::remove_dir(&canonical_dir) {
            log::warn!("delete_staged: prune empty dir {canonical_dir:?} failed: {e}");
        }
    }

    Ok(())
}
