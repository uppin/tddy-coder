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

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tddy_rpc::Status;

use crate::session_file_upload::{contained_canonical_dir, validate_segment};

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

fn mtime_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn staged_complete_marker(canonical_dir: &Path, file_name: &str) -> PathBuf {
    canonical_dir.join(format!("{file_name}{STAGED_COMPLETE_SUFFIX}"))
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
    let safe_staging = validate_segment(staging_id)?;
    let safe_name = validate_segment(file_name)?;

    std::fs::create_dir_all(staging_root).map_err(|e| {
        log::error!(
            "write_staged_chunk: create_dir_all {:?} failed: {}",
            staging_root,
            e
        );
        Status::internal(format!("failed to create staging root: {}", e))
    })?;

    let dir = staging_root.join(safe_staging);
    std::fs::create_dir_all(&dir).map_err(|e| {
        log::error!("write_staged_chunk: create_dir_all {:?} failed: {}", dir, e);
        Status::internal(format!("failed to create staging batch dir: {}", e))
    })?;

    let canonical_dir = contained_canonical_dir(staging_root, &dir)?;

    let complete_marker = staged_complete_marker(&canonical_dir, safe_name);
    if complete_marker.exists() {
        log::warn!(
            "write_staged_chunk: refusing to overwrite completed staged file {:?}",
            safe_name
        );
        return Err(Status::invalid_argument(
            "staged file already exists in this batch",
        ));
    }

    let target = canonical_dir.join(safe_name);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .map_err(|e| {
            log::error!("write_staged_chunk: open {:?} failed: {}", target, e);
            Status::internal(format!("failed to open staged file target: {}", e))
        })?;
    file.write_all(data).map_err(|e| {
        log::error!("write_staged_chunk: write {:?} failed: {}", target, e);
        Status::internal(format!("failed to write staged chunk: {}", e))
    })?;

    if last {
        std::fs::write(&complete_marker, b"").map_err(|e| {
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
    staging_id: &str,
    batch_path: &Path,
    out: &mut Vec<StagedAttachmentFile>,
) -> Result<(), Status> {
    let files = fs::read_dir(batch_path).map_err(|e| {
        log::error!("list_staged_attachments: read_dir {batch_path:?} failed: {e}");
        Status::internal(format!("failed to read staging batch dir: {e}"))
    })?;
    for file in files {
        let file = file.map_err(|e| {
            log::error!("list_staged_attachments: read_dir entry in {batch_path:?} failed: {e}");
            Status::internal(format!("failed to read staging batch dir: {e}"))
        })?;
        let name = file.file_name().to_string_lossy().into_owned();
        if is_staged_complete_marker_name(&name) {
            continue;
        }
        let metadata = match file.metadata() {
            Ok(m) if m.is_file() => m,
            Ok(_) => continue,
            Err(e) => {
                log::warn!(
                    "list_staged_attachments: metadata {:?} failed: {e}",
                    file.path()
                );
                continue;
            }
        };
        out.push(StagedAttachmentFile {
            staging_id: staging_id.to_string(),
            file_name: name,
            host_path: file.path(),
            size_bytes: metadata.len(),
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
    let root_entries = match fs::read_dir(staging_root) {
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
        if batch_path.is_dir() {
            let canonical_dir = contained_canonical_dir(staging_root, &batch_path)?;
            collect_batch_files(safe_staging, &canonical_dir, &mut files)?;
        }
    } else {
        for batch_dir in root_entries {
            let batch_dir = batch_dir.map_err(|e| {
                log::error!(
                    "list_staged_attachments: read_dir entry in {staging_root:?} failed: {e}"
                );
                Status::internal(format!("failed to read staging root: {e}"))
            })?;
            if !batch_dir.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let batch_staging_id = batch_dir.file_name().to_string_lossy().into_owned();
            let batch_path = batch_dir.path();
            let canonical_dir = match contained_canonical_dir(staging_root, &batch_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            collect_batch_files(&batch_staging_id, &canonical_dir, &mut files)?;
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
    let safe_staging = validate_segment(staging_id)?;
    let safe_name = validate_segment(file_name)?;

    let dir = staging_root.join(safe_staging);
    if !dir.exists() {
        return Err(Status::not_found("staged file not found"));
    }

    let canonical_dir = contained_canonical_dir(staging_root, &dir)?;
    let target = canonical_dir.join(safe_name);
    if !target.is_file() {
        return Err(Status::not_found("staged file not found"));
    }

    fs::remove_file(&target).map_err(|e| {
        log::error!("delete_staged_attachment: remove_file {target:?} failed: {e}");
        Status::internal(format!("failed to delete staged file: {e}"))
    })?;

    let marker = staged_complete_marker(&canonical_dir, safe_name);
    if marker.exists() {
        if let Err(e) = fs::remove_file(&marker) {
            log::warn!("delete_staged_attachment: remove marker {marker:?} failed: {e}");
        }
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tddy_rpc::Code;

    const SESSION_FILE: &str = "report.pdf";
    const STAGING_ID: &str = "11111111-1111-7111-8111-111111111111";

    fn a_staging_root() -> (tempfile::TempDir, PathBuf) {
        let os_user = std::env::var("USER").expect("USER");
        let data = tempfile::tempdir().unwrap();
        let root = staging_root_for(&os_user, data.path());
        std::fs::create_dir_all(&root).unwrap();
        (data, root)
    }

    /// AC(staging-1) — ordered chunks reassemble into one file; only the final chunk returns a path.
    #[test]
    fn write_staged_chunk_appends_ordered_chunks_and_returns_the_path_on_the_final_chunk() {
        // Given — an empty staging root
        let (_data, root) = a_staging_root();

        // When — a file arrives as two ordered chunks, the second marked final
        let first = write_staged_chunk(&root, STAGING_ID, SESSION_FILE, b"Hel", false).unwrap();
        let last = write_staged_chunk(&root, STAGING_ID, SESSION_FILE, b"lo!", true).unwrap();

        // Then — no path until the final chunk, then the absolute path, holding the reassembled bytes
        assert_eq!(first, None, "non-final chunk returns no path");
        let host_path = last.expect("final chunk returns the absolute host path");
        assert!(host_path.is_absolute());
        assert_eq!(std::fs::read(&host_path).unwrap(), b"Hello!");
    }

    /// AC(staging-2) — an unsafe `staging_id` or `file_name` is rejected with INVALID_ARGUMENT and
    /// writes nothing.
    #[test]
    fn write_staged_chunk_rejects_an_unsafe_staging_id_or_file_name_as_invalid_argument() {
        // Given
        let (_data, root) = a_staging_root();

        // When / Then — a traversal staging_id, a nested file_name, and an empty file_name are all refused
        let trav = write_staged_chunk(&root, "../escape", SESSION_FILE, b"x", true).unwrap_err();
        assert_eq!(trav.code, Code::InvalidArgument);
        let nested = write_staged_chunk(&root, STAGING_ID, "sub/evil.txt", b"x", true).unwrap_err();
        assert_eq!(nested.code, Code::InvalidArgument);
        let empty = write_staged_chunk(&root, STAGING_ID, "", b"x", true).unwrap_err();
        assert_eq!(empty.code, Code::InvalidArgument);
        assert!(
            !root.join(STAGING_ID).exists(),
            "nothing written for a rejected segment"
        );
    }

    /// AC(staging-3) — re-uploading the same file_name within one batch is refused (no silent overwrite).
    #[test]
    fn write_staged_chunk_refuses_to_overwrite_an_existing_file_name_within_a_batch() {
        // Given — a batch already holding "report.pdf"
        let (_data, root) = a_staging_root();
        write_staged_chunk(&root, STAGING_ID, SESSION_FILE, b"first", true).unwrap();

        // When / Then — a second upload of the same name into the same batch is rejected
        let err = write_staged_chunk(&root, STAGING_ID, SESSION_FILE, b"second", true).unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
        assert_eq!(
            std::fs::read(root.join(STAGING_ID).join(SESSION_FILE)).unwrap(),
            b"first",
            "the original file must remain untouched"
        );
    }

    /// AC(staging-4) — `list_staged_attachments` with a `staging_id` returns that batch's files,
    /// newest-first by modification time.
    #[test]
    fn list_staged_attachments_returns_one_batch_newest_first_when_staging_id_is_set() {
        // Given — one batch with two files written at slightly different mtimes
        let (_data, root) = a_staging_root();
        write_staged_chunk(&root, STAGING_ID, "a.txt", b"a", true).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_staged_chunk(&root, STAGING_ID, "b.txt", b"b", true).unwrap();

        // When
        let files = list_staged_attachments(&root, STAGING_ID).unwrap();

        // Then — newest first (b before a), both under the requested batch
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_name, "b.txt");
        assert_eq!(files[1].file_name, "a.txt");
        assert_eq!(files[0].staging_id, STAGING_ID);
    }

    /// AC(staging-5) — an empty `staging_id` lists every batch for the caller, newest-first.
    #[test]
    fn list_staged_attachments_returns_every_batch_for_the_caller_when_staging_id_is_empty() {
        // Given — two batches, the second written later
        let (_data, root) = a_staging_root();
        let older = "22222222-2222-7222-8222-222222222222";
        write_staged_chunk(&root, older, "old.txt", b"o", true).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_staged_chunk(&root, STAGING_ID, "new.txt", b"n", true).unwrap();

        // When
        let files = list_staged_attachments(&root, "").unwrap();

        // Then — the newer batch's file sorts first
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].staging_id, STAGING_ID);
        assert_eq!(files[1].staging_id, older);
    }

    /// AC(staging-6) — delete removes one file and rejects an unsafe segment.
    #[test]
    fn delete_staged_attachment_removes_one_file_and_rejects_an_unsafe_segment() {
        // Given — a batch with one file
        let (_data, root) = a_staging_root();
        write_staged_chunk(&root, STAGING_ID, SESSION_FILE, b"x", true).unwrap();

        // When
        delete_staged_attachment(&root, STAGING_ID, SESSION_FILE).unwrap();

        // Then — the file is gone
        assert!(!root.join(STAGING_ID).join(SESSION_FILE).exists());

        // When / Then — a traversal delete is refused (no weaker gate than the writer)
        let err = delete_staged_attachment(&root, "../escape", SESSION_FILE).unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }

    /// AC(staging-7) — a per-batch directory that is a symlink escaping the staging root is refused.
    #[test]
    fn a_staging_directory_symlinked_outside_the_staging_root_is_refused() {
        // Given — a symlink {staging_root}/evil -> an outside dir
        let (_data, root) = a_staging_root();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("evil")).unwrap();

        // When / Then — writing through the symlinked batch dir is refused
        let err = write_staged_chunk(&root, "evil", SESSION_FILE, b"x", true).unwrap_err();
        assert_eq!(err.code, Code::InvalidArgument);
    }
}
