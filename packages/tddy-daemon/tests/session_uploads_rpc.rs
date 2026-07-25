//! `ConnectionService.ListSessionUploads` / `DeleteSessionUpload` — the host side of the Session
//! Inspector Files tab. The tab lists the files already uploaded to a session
//! (`{session_dir}/uploads/{upload_id}/{file_name}`, written by `write_upload_chunk`) so they are
//! repeatedly usable, and deletes them on demand.
//!
//! These pin the flat newest-first listing across `upload_id` folders, the empty-when-absent case,
//! the delete + empty-folder pruning, the shared basename / traversal guard (delete must be no
//! weaker than write), and the unauthenticated-token rejection at the RPC boundary.
//!
//! PRD: docs/ft/web/session-files-inspector.md

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::session_file_upload::write_upload_chunk;
use tddy_daemon::session_uploads::{delete_upload, list_uploads};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionUploadRequest,
    ListSessionUploadsRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const SESSION_ID: &str = "11111111-1111-7111-8111-111111111111";
const UPLOAD_A: &str = "22222222-2222-7222-8222-222222222222";
const UPLOAD_B: &str = "33333333-3333-7333-8333-333333333333";

/// The directory a drop's files land in: `{base}/sessions/{session_id}/uploads/{upload_id}`.
fn expected_upload_dir(base: &Path, session_id: &str, upload_id: &str) -> PathBuf {
    base.join("sessions")
        .join(session_id)
        .join("uploads")
        .join(upload_id)
}

/// Write a completed one-chunk upload and return its absolute host path.
fn upload_one(base: &Path, upload_id: &str, file_name: &str, bytes: &[u8]) -> PathBuf {
    write_upload_chunk(base, SESSION_ID, upload_id, file_name, bytes, true)
        .unwrap()
        .unwrap()
}

/// Pin a file's modification time so listing order (newest first) is deterministic.
fn set_mtime(path: &Path, time: SystemTime) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

fn at_secs(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

// ---------------------------------------------------------------------------
// Pure module: list_uploads
// ---------------------------------------------------------------------------

#[test]
fn lists_uploaded_files_across_upload_folders_newest_first() {
    // Given — two files under two different drop folders, with pinned mtimes
    let base = tempfile::tempdir().unwrap();
    let older = upload_one(base.path(), UPLOAD_A, "older.txt", b"a");
    let newer = upload_one(base.path(), UPLOAD_B, "newer.txt", b"bb");
    set_mtime(&older, at_secs(1_000));
    set_mtime(&newer, at_secs(2_000));

    // When
    let uploads = list_uploads(base.path(), SESSION_ID).unwrap();

    // Then — one flat list, newest mtime first, with the mtime surfaced in milliseconds
    let names: Vec<&str> = uploads.iter().map(|u| u.file_name.as_str()).collect();
    assert_eq!(names, vec!["newer.txt", "older.txt"]);
    assert_eq!(uploads[0].uploaded_at_ms, 2_000_000);
    assert_eq!(uploads[1].uploaded_at_ms, 1_000_000);
}

#[test]
fn reports_each_files_absolute_host_path_and_size() {
    // Given — one uploaded file of known length
    let base = tempfile::tempdir().unwrap();
    upload_one(base.path(), UPLOAD_A, "report.pdf", b"Hello!");

    // When
    let uploads = list_uploads(base.path(), SESSION_ID).unwrap();

    // Then — the entry carries the absolute host path and exact byte size
    assert_eq!(uploads.len(), 1);
    let entry = &uploads[0];
    assert_eq!(entry.upload_id, UPLOAD_A);
    assert_eq!(entry.file_name, "report.pdf");
    assert!(entry.host_path.is_absolute(), "host path must be absolute");
    assert_eq!(
        entry.host_path,
        expected_upload_dir(base.path(), SESSION_ID, UPLOAD_A).join("report.pdf")
    );
    assert_eq!(entry.size_bytes, 6);
}

#[test]
fn returns_an_empty_list_when_no_uploads_directory_exists() {
    // Given — a session that never had an upload (no uploads dir)
    let base = tempfile::tempdir().unwrap();

    // When
    let uploads = list_uploads(base.path(), SESSION_ID).unwrap();

    // Then — empty, not an error
    assert!(uploads.is_empty());
}

// ---------------------------------------------------------------------------
// Pure module: delete_upload
// ---------------------------------------------------------------------------

#[test]
fn delete_upload_removes_the_file() {
    // Given — one uploaded file
    let base = tempfile::tempdir().unwrap();
    let path = upload_one(base.path(), UPLOAD_A, "report.pdf", b"x");
    assert!(path.exists());

    // When
    delete_upload(base.path(), SESSION_ID, UPLOAD_A, "report.pdf").unwrap();

    // Then — the file is gone
    assert!(!path.exists());
}

#[test]
fn delete_upload_prunes_the_emptied_upload_folder() {
    // Given — a drop folder holding a single file
    let base = tempfile::tempdir().unwrap();
    upload_one(base.path(), UPLOAD_A, "only.txt", b"x");
    let dir = expected_upload_dir(base.path(), SESSION_ID, UPLOAD_A);

    // When — that file is deleted
    delete_upload(base.path(), SESSION_ID, UPLOAD_A, "only.txt").unwrap();

    // Then — the now-empty drop folder is pruned
    assert!(!dir.exists(), "emptied upload_id folder should be pruned");
}

#[test]
fn delete_upload_keeps_sibling_files_in_the_same_folder() {
    // Given — two files under one drop folder
    let base = tempfile::tempdir().unwrap();
    upload_one(base.path(), UPLOAD_A, "keep.txt", b"k");
    let removed = upload_one(base.path(), UPLOAD_A, "drop.txt", b"d");
    let kept = expected_upload_dir(base.path(), SESSION_ID, UPLOAD_A).join("keep.txt");

    // When — one is deleted
    delete_upload(base.path(), SESSION_ID, UPLOAD_A, "drop.txt").unwrap();

    // Then — the sibling and its folder remain
    assert!(!removed.exists());
    assert!(kept.exists(), "sibling file must survive");
}

#[test]
fn delete_upload_of_a_missing_file_reports_not_found() {
    // Given — an empty session uploads root
    let base = tempfile::tempdir().unwrap();
    upload_one(base.path(), UPLOAD_A, "present.txt", b"p");

    // When / Then — deleting a name that was never uploaded is NotFound
    let err = delete_upload(base.path(), SESSION_ID, UPLOAD_A, "absent.txt").unwrap_err();
    assert_eq!(err.code, Code::NotFound);
}

#[test]
fn delete_upload_rejects_a_file_name_with_a_path_separator() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then — a nested name could escape the flat per-drop folder
    let err = delete_upload(base.path(), SESSION_ID, UPLOAD_A, "sub/evil.txt").unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn delete_upload_rejects_a_parent_traversal_upload_id() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then — the upload_id is untrusted client input and must be a safe basename
    let err = delete_upload(base.path(), SESSION_ID, "../escape", "note.txt").unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn delete_upload_writes_nothing_and_removes_nothing_outside_the_uploads_dir_when_rejected() {
    // Given — a sentinel file above the uploads root that a traversal must not touch
    let base = tempfile::tempdir().unwrap();
    let sentinel = expected_upload_dir(base.path(), SESSION_ID, UPLOAD_A);
    std::fs::create_dir_all(&sentinel).unwrap();
    let outside = base.path().join("outside.txt");
    std::fs::write(&outside, b"keep me").unwrap();

    // When — a traversal file_name is rejected
    let err = delete_upload(base.path(), SESSION_ID, UPLOAD_A, "../../../outside.txt").unwrap_err();

    // Then — rejected, and nothing outside the uploads root was removed
    assert_eq!(err.code, Code::InvalidArgument);
    assert!(outside.exists(), "delete must not escape the uploads root");
}

// ---------------------------------------------------------------------------
// RPC boundary: ListSessionUploads / DeleteSessionUpload
// ---------------------------------------------------------------------------

fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(sessions_base: PathBuf, os_user: &str) -> ConnectionServiceImpl {
    let config = test_config_for_os_user(os_user);
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

#[tokio::test]
async fn list_session_uploads_returns_previously_uploaded_files() {
    // Given — a service whose sessions base already holds two uploaded files
    let os_user = std::env::var("USER").expect("USER must be set");
    let base = tempfile::tempdir().unwrap();
    upload_one(base.path(), UPLOAD_A, "report.pdf", b"Hello!");
    upload_one(base.path(), UPLOAD_B, "diagram.png", b"PNG");
    let service = test_service(base.path().to_path_buf(), &os_user);

    // When
    let resp = service
        .list_session_uploads(Request::new(ListSessionUploadsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then — both uploads are returned
    let mut names: Vec<&str> = resp.uploads.iter().map(|u| u.file_name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["diagram.png", "report.pdf"]);
}

#[tokio::test]
async fn list_session_uploads_rejects_an_invalid_session_token() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .list_session_uploads(Request::new(ListSessionUploadsRequest {
            session_token: "bad".to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}

#[tokio::test]
async fn delete_session_upload_rejects_an_invalid_session_token() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .delete_session_upload(Request::new(DeleteSessionUploadRequest {
            session_token: "bad".to_string(),
            session_id: SESSION_ID.to_string(),
            upload_id: UPLOAD_A.to_string(),
            file_name: "note.txt".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}
