//! A session's name in `.session.yaml`: the rules every writer applies to a requested name, and
//! that renaming a session changes its name and nothing else.
//!
//! Feature doc: docs/ft/web/session-drawer.md § Session names.

use pretty_assertions::assert_eq;
use tddy_changeset::session_display_name::{
    normalize_display_name, rename_session, DisplayNameError, RenameSessionError,
    DISPLAY_NAME_MAX_CHARS,
};
use tddy_changeset::{read_session_metadata, write_session_metadata, SessionMetadata};

const UPDATED_AT: &str = "2026-11-10T12:00:00Z";

fn a_session() -> SessionMetadata {
    serde_yaml::from_str(&format!(
        r#"session_id: 018f1234-5678-7abc-8def-123456789abc
project_id: proj-a
created_at: "2026-11-10T12:00:00Z"
updated_at: "{UPDATED_AT}"
status: active
session_type: workspace
repo_path: /worktrees/proj-a/feature-login
"#
    ))
    .expect("the fixture is a .session.yaml SessionMetadata accepts")
}

fn a_recorded_session() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_session_metadata(dir.path(), &a_session()).unwrap();
    dir
}

fn recorded_name(dir: &tempfile::TempDir) -> Option<String> {
    read_session_metadata(dir.path()).unwrap().display_name
}

// ── the rules a requested name is held to ────────────────────────────────────────────────────

#[test]
fn a_name_is_recorded_without_its_surrounding_whitespace() {
    // When / Then
    assert_eq!(
        normalize_display_name("  Fix the login flow \n"),
        Ok(Some("Fix the login flow".to_string()))
    );
}

#[test]
fn an_empty_or_blank_name_is_no_name() {
    // When / Then
    assert_eq!(
        (normalize_display_name(""), normalize_display_name(" \t ")),
        (Ok(None), Ok(None))
    );
}

#[test]
fn the_limit_counts_characters_not_bytes() {
    // Given a name of exactly the maximum length in two-byte characters
    let at_the_limit = "é".repeat(DISPLAY_NAME_MAX_CHARS);

    // When / Then it is kept whole
    assert_eq!(
        normalize_display_name(&at_the_limit),
        Ok(Some(at_the_limit.clone()))
    );
}

#[test]
fn a_name_over_the_limit_is_refused_naming_both_lengths() {
    // Given
    let over = "x".repeat(DISPLAY_NAME_MAX_CHARS + 1);

    // When
    let refused = normalize_display_name(&over).unwrap_err();

    // Then it is refused rather than cut, and the refusal says by how much
    assert_eq!(
        refused,
        DisplayNameError::TooLong {
            chars: DISPLAY_NAME_MAX_CHARS + 1,
            max: DISPLAY_NAME_MAX_CHARS
        }
    );
    let message = refused.to_string();
    assert!(
        message.contains("81") && message.contains("80"),
        "{message}"
    );
}

#[test]
fn a_name_is_one_line() {
    // When / Then — a control character inside the name is refused, not cleaned
    assert_eq!(
        (
            normalize_display_name("Fix the\nlogin flow"),
            normalize_display_name("Fix\tthe login flow"),
        ),
        (
            Err(DisplayNameError::ControlCharacter),
            Err(DisplayNameError::ControlCharacter)
        )
    );
}

// ── renaming a recorded session ──────────────────────────────────────────────────────────────

#[test]
fn renaming_records_the_name_and_changes_nothing_else() {
    // Given a recorded session
    let dir = a_recorded_session();
    let before = read_session_metadata(dir.path()).unwrap();

    // When it is renamed
    let recorded = rename_session(dir.path(), "  Fix the login flow ").unwrap();

    // Then the trimmed name is recorded and answered, and every other field — `updated_at`
    // included, since naming a session is not activity in it — is as it was
    let after = read_session_metadata(dir.path()).unwrap();
    assert_eq!(recorded.as_deref(), Some("Fix the login flow"));
    assert_eq!(after.display_name.as_deref(), Some("Fix the login flow"));
    assert_eq!(
        serde_yaml::to_value(SessionMetadata {
            display_name: None,
            ..after
        })
        .unwrap(),
        serde_yaml::to_value(before).unwrap()
    );
}

#[test]
fn renaming_to_nothing_clears_the_name_and_writes_no_key() {
    // Given a named session
    let dir = a_recorded_session();
    rename_session(dir.path(), "Fix the login flow").unwrap();

    // When it is renamed to blank
    let recorded = rename_session(dir.path(), "   ").unwrap();

    // Then it has no name, and its record reads as one never named
    let yaml = std::fs::read_to_string(dir.path().join(".session.yaml")).unwrap();
    assert_eq!((recorded, recorded_name(&dir)), (None, None));
    assert!(!yaml.contains("display_name"), "{yaml}");
}

#[test]
fn a_refused_name_leaves_the_recorded_one() {
    // Given a named session
    let dir = a_recorded_session();
    rename_session(dir.path(), "Fix the login flow").unwrap();

    // When it is renamed to a name over the limit
    let refused = rename_session(dir.path(), &"x".repeat(DISPLAY_NAME_MAX_CHARS + 1));

    // Then the rename is refused as invalid, and the old name stands
    assert!(
        matches!(
            refused,
            Err(RenameSessionError::Invalid(
                DisplayNameError::TooLong { .. }
            ))
        ),
        "{refused:?}"
    );
    assert_eq!(recorded_name(&dir).as_deref(), Some("Fix the login flow"));
}

#[test]
fn renaming_a_directory_with_no_record_is_refused_as_unreadable() {
    // Given a directory no session was recorded in
    let dir = tempfile::tempdir().unwrap();

    // When / Then
    assert!(
        matches!(
            rename_session(dir.path(), "Fix the login flow"),
            Err(RenameSessionError::Unreadable(_))
        ),
        "a missing record is reported as unreadable"
    );
}

#[test]
fn a_record_from_before_names_reads_as_unnamed() {
    // Given a `.session.yaml` written before sessions had names
    let dir = a_recorded_session();

    // When / Then
    assert_eq!(recorded_name(&dir), None);
}
