//! Staging-area tests over a real staging root on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
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
