//! Unit: the `STAGED_ATTACHMENT` scope's `relative_path` shape.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md`
//! Changeset: `docs/dev/1-WIP/2026-08-01-session-attach-ui.md`
//!
//! The scope addresses a staged file as `<staging_id>/<file_name>` — the same two-segment shape
//! `SESSION_UPLOAD` uses for `<upload_id>/<file_name>`. Both segments are untrusted client input
//! that become path components, so each must be a pure basename. These pin the parser in isolation;
//! `tests/session_attach_staging_scope_acceptance.rs` pins the same rules through the RPC.

use tddy_daemon::host_documents::validate_staged_attachment_relative_path;
use tddy_rpc::Code;

const STAGING_ID: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";

/// The exact `Status` code and message for a refused path, so a refusal for the wrong reason fails.
fn refusal_of(relative_path: &str) -> (Code, String) {
    let err =
        validate_staged_attachment_relative_path(relative_path).expect_err("path must be refused");
    (err.code, err.message)
}

#[test]
fn accepts_a_staging_id_and_file_name() {
    // Given / When
    let result = validate_staged_attachment_relative_path(&format!("{STAGING_ID}/spec.md"));

    // Then
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn refuses_an_empty_relative_path() {
    // Given / When
    let (code, message) = refusal_of("");

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must not be empty");
}

#[test]
fn refuses_a_bare_file_name_naming_no_batch() {
    // Given / When
    let (code, message) = refusal_of("spec.md");

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(
        message,
        "staged attachment relative_path must be <staging_id>/<file_name>"
    );
}

#[test]
fn refuses_a_third_segment() {
    // Given / When
    let (code, message) = refusal_of(&format!("{STAGING_ID}/nested/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(
        message,
        "staged attachment relative_path must be <staging_id>/<file_name>"
    );
}

#[test]
fn refuses_a_trailing_separator_that_leaves_an_empty_file_name() {
    // Given / When
    let (code, _message) = refusal_of(&format!("{STAGING_ID}/"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
}

#[test]
fn refuses_an_absolute_path() {
    // Given / When
    let (code, message) = refusal_of(&format!("/{STAGING_ID}/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must be relative");
}

#[test]
fn refuses_a_path_that_traverses_out_of_its_batch() {
    // Given / When
    let (code, message) = refusal_of(&format!("{STAGING_ID}/../other/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must not traverse");
}

#[test]
fn refuses_a_current_directory_segment() {
    // Given / When
    let (code, message) = refusal_of(&format!("./{STAGING_ID}/spec.md"));

    // Then
    assert_eq!(code, Code::InvalidArgument);
    assert_eq!(message, "relative_path must not contain '.' segments");
}
