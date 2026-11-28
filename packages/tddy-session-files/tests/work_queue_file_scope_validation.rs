//! Unit: the `WORK_QUEUE_FILE` scope's `relative_path` shape and its resolution root.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`
//!
//! A work item's file is addressed as `<item_id>/<file_name>` — the two-segment shape
//! `SESSION_UPLOAD` and `STAGED_ATTACHMENT` use, and for the same reason: both segments are
//! untrusted client input that become path components. These pin the shape rules in isolation and
//! the root the scope resolves to; the cross-host fetch through the RPC is pinned by the spawn
//! acceptance tests.

use std::path::Path;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Code;
use tddy_service::proto::types::HostDocumentScope;
use tddy_session_files::host_documents::{resolve_host_document, validate_relative_path};

const OWNER_SESSION: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
const ITEM: &str = "n1";

/// The exact `Status` code and message for a refused path, so a refusal for the wrong reason fails.
fn refusal_of(relative_path: &str) -> (Code, String) {
    let err = validate_relative_path(HostDocumentScope::WorkQueueFile, relative_path)
        .expect_err("path must be refused");
    (err.code, err.message)
}

#[test]
fn accepts_an_item_id_and_file_name() {
    // Given / When
    let result =
        validate_relative_path(HostDocumentScope::WorkQueueFile, &format!("{ITEM}/spec.md"));

    // Then
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn refuses_a_bare_file_name_naming_no_item() {
    // Given / When
    let (code, message) = refusal_of("spec.md");

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(
        message,
        "work queue file relative_path must be <item_id>/<file_name>"
    );
}

#[test]
fn refuses_a_third_segment() {
    // Given / When
    let (code, message) = refusal_of(&format!("{ITEM}/nested/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(
        message,
        "work queue file relative_path must be <item_id>/<file_name>"
    );
}

#[test]
fn refuses_an_empty_relative_path() {
    // Given / When
    let (code, _) = refusal_of("");

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[test]
fn refuses_an_absolute_path() {
    // Given / When
    let (code, message) = refusal_of(&format!("/{ITEM}/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must be relative");
}

#[test]
fn refuses_a_path_that_traverses_out_of_its_item() {
    // Given / When
    let (code, message) = refusal_of(&format!("{ITEM}/../other/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must not traverse");
}

#[test]
fn refuses_a_current_directory_segment() {
    // Given / When
    let (code, _) = refusal_of(&format!("./{ITEM}/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[test]
fn resolves_under_the_owning_sessions_work_queue_files_directory() {
    // Given — the owner host holds the file under its session's directory
    let data = tempfile::tempdir().expect("temp data dir");
    let session_dir = unified_session_dir_path(data.path(), OWNER_SESSION);
    let item_dir = session_dir.join("work-queue-files").join(ITEM);
    std::fs::create_dir_all(&item_dir).expect("item dir");
    std::fs::write(item_dir.join("spec.md"), b"# the spec\n").expect("file");

    // When
    let resolved = resolve_host_document(
        std::env::var("USER").expect("USER").as_str(),
        data.path(),
        Path::new("/nonexistent-staging-base"),
        HostDocumentScope::WorkQueueFile,
        OWNER_SESSION,
        "",
        &format!("{ITEM}/spec.md"),
    )
    .expect("resolved");

    // Then
    assert_eq!(resolved.byte_size, 11);
    assert!(
        resolved
            .path
            .ends_with(Path::new("work-queue-files").join(ITEM).join("spec.md")),
        "resolved to {:?}",
        resolved.path
    );
}

#[test]
fn a_file_outside_the_scope_root_is_refused_even_when_it_exists() {
    // Given — a real file, but under the session dir, not under the scope root
    let data = tempfile::tempdir().expect("temp data dir");
    let session_dir = unified_session_dir_path(data.path(), OWNER_SESSION);
    std::fs::create_dir_all(session_dir.join("secrets")).expect("other dir");
    std::fs::write(session_dir.join("secrets").join("spec.md"), b"no").expect("file");

    // When
    let err = resolve_host_document(
        std::env::var("USER").expect("USER").as_str(),
        data.path(),
        Path::new("/nonexistent-staging-base"),
        HostDocumentScope::WorkQueueFile,
        OWNER_SESSION,
        "",
        // A path that would resolve inside the root's *parent* if the containment gate were lenient.
        &format!("{ITEM}/../secrets/spec.md"),
    )
    .expect_err("must be refused");

    // Then
    assert_eq!(err.code, Code::InvalidArgument);
    assert_eq!(err.message, "relative_path must not traverse");
}
