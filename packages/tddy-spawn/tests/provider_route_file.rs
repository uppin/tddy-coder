//! The daemon writes a tool session's route file owner-only, beside the session, and the child is
//! told where it is.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md

use std::os::unix::fs::PermissionsExt;

use tddy_spawn::spawner::write_provider_route_file;

fn this_user() -> String {
    let out = std::process::Command::new("id")
        .arg("-un")
        .output()
        .expect("id -un");
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
}

#[test]
fn a_route_file_is_written_owner_only_in_the_session_directory() {
    // Given
    let session_dir = tempfile::tempdir().expect("dir");

    // When
    let path = write_provider_route_file(session_dir.path(), r#"{"env":[]}"#, &this_user())
        .expect("written");

    // Then
    assert_eq!(path, session_dir.path().join("provider-route.json"));
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        r#"{"env":[]}"#
    );
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn rewriting_a_route_file_resecures_it() {
    // Given a previous run left the file world-readable
    let session_dir = tempfile::tempdir().expect("dir");
    let stale = session_dir.path().join("provider-route.json");
    std::fs::write(&stale, "{}").expect("stale");
    std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o644)).expect("chmod");

    // When
    write_provider_route_file(session_dir.path(), r#"{"env":[]}"#, &this_user()).expect("written");

    // Then
    let mode = std::fs::metadata(&stale)
        .expect("meta")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
