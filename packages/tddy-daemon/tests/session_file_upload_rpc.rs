//! `ConnectionService.UploadSessionFileChunk` — the host side of the terminal drag-to-upload
//! feature. Files dropped on the web terminal stream to the host in ordered chunks and land under
//! `{session_dir}/uploads/{upload_id}/{file_name}`; the daemon returns each file's absolute host
//! path on its final chunk so the web can type that path into the terminal.
//!
//! These pin the chunk-append + host-path contract, the per-`upload_id` isolation, the basename /
//! traversal guard, and the unauthenticated-token rejection.
//!
//! PRD: docs/ft/web/web-terminal.md § File drop upload

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::session_file_upload::write_upload_chunk;
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, UploadSessionFileChunkRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const SESSION_ID: &str = "11111111-1111-7111-8111-111111111111";
const UPLOAD_ID: &str = "22222222-2222-7222-8222-222222222222";

// ---------------------------------------------------------------------------
// Pure module: write_upload_chunk
// ---------------------------------------------------------------------------

/// The directory a drop's files land in: `{base}/sessions/{session_id}/uploads/{upload_id}`.
fn expected_upload_dir(base: &std::path::Path, session_id: &str, upload_id: &str) -> PathBuf {
    base.join("sessions")
        .join(session_id)
        .join("uploads")
        .join(upload_id)
}

#[test]
fn appends_chunks_in_order_and_returns_the_absolute_host_path_on_the_last_chunk() {
    // Given — a session data-dir base
    let base = tempfile::tempdir().unwrap();

    // When — a file arrives as two ordered chunks, the second marked final
    let first = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "report.pdf",
        b"Hel",
        false,
    )
    .unwrap();
    let last = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "report.pdf",
        b"lo!",
        true,
    )
    .unwrap();

    // Then — no path until the final chunk, then the absolute host path, and the file holds the
    // reassembled bytes in order
    assert_eq!(first, None, "non-final chunk returns no host path");
    let host_path = last.expect("final chunk returns the absolute host path");
    assert_eq!(
        host_path,
        expected_upload_dir(base.path(), SESSION_ID, UPLOAD_ID).join("report.pdf")
    );
    assert!(host_path.is_absolute(), "host path must be absolute");
    assert_eq!(std::fs::read(&host_path).unwrap(), b"Hello!");
}

#[test]
fn keeps_each_drops_files_in_its_own_upload_id_folder() {
    // Given — the same file name uploaded under two different drop ids
    let base = tempfile::tempdir().unwrap();
    let other_upload = "33333333-3333-7333-8333-333333333333";

    // When
    let a = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "notes.txt",
        b"first",
        true,
    )
    .unwrap()
    .unwrap();
    let b = write_upload_chunk(
        base.path(),
        SESSION_ID,
        other_upload,
        "notes.txt",
        b"second",
        true,
    )
    .unwrap()
    .unwrap();

    // Then — two distinct files, neither overwriting the other
    assert_ne!(a, b);
    assert_eq!(std::fs::read(&a).unwrap(), b"first");
    assert_eq!(std::fs::read(&b).unwrap(), b"second");
}

#[test]
fn rejects_a_file_name_containing_a_path_separator() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then — a nested name would escape the flat per-drop folder
    let err = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "sub/evil.txt",
        b"x",
        true,
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn rejects_a_parent_traversal_file_name() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then
    let err = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "../escape.txt",
        b"x",
        true,
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn rejects_a_parent_traversal_upload_id() {
    // Given — a crafted upload_id that would climb out of the session's uploads dir
    let base = tempfile::tempdir().unwrap();

    // When / Then — the upload_id is untrusted client input and must be a safe basename
    let err = write_upload_chunk(base.path(), SESSION_ID, "../escape", "note.txt", b"x", true)
        .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn rejects_an_upload_id_containing_a_path_separator() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then
    let err =
        write_upload_chunk(base.path(), SESSION_ID, "a/b", "note.txt", b"x", true).unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn rejects_an_empty_file_name() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When / Then
    let err = write_upload_chunk(base.path(), SESSION_ID, UPLOAD_ID, "", b"x", true).unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

#[test]
fn writes_nothing_outside_the_uploads_dir_when_the_name_is_rejected() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When — a traversal name is rejected
    let _ = write_upload_chunk(
        base.path(),
        SESSION_ID,
        UPLOAD_ID,
        "../escape.txt",
        b"x",
        true,
    );

    // Then — nothing was written anywhere under the base
    assert!(
        !base.path().join("escape.txt").exists(),
        "rejected upload must not write outside the uploads dir"
    );
    assert!(!expected_upload_dir(base.path(), SESSION_ID, UPLOAD_ID)
        .join("escape.txt")
        .exists());
}

// ---------------------------------------------------------------------------
// RPC boundary: UploadSessionFileChunk auth
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

/// Acceptance: an invalid session token is rejected before any filesystem access.
#[tokio::test]
async fn upload_session_file_chunk_rejects_an_invalid_session_token() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .upload_session_file_chunk(Request::new(UploadSessionFileChunkRequest {
            session_token: "bad".to_string(),
            session_id: SESSION_ID.to_string(),
            upload_id: UPLOAD_ID.to_string(),
            file_name: "note.txt".to_string(),
            data: b"x".to_vec(),
            last: true,
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}
