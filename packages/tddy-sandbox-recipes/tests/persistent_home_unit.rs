//! Unit: persistent tool homes live at `<tddyhome>/homes/<key>`, and a legacy
//! `<tddyhome>/sandbox-<key>-home` is moved there once.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_sandbox_recipes::homes::{persistent_home, HomeError};

const CREDENTIALS: &str = r#"{"claudeAiOauth":{"accessToken":"kept-across-the-move"}}"#;

/// `<tddy_home>/sandbox-claude-home` holding a credentials file, as a recipe left it.
fn a_legacy_claude_home_with_credentials(tddy_home: &Path) -> PathBuf {
    let legacy = tddy_home.join("sandbox-claude-home");
    std::fs::create_dir_all(legacy.join(".claude")).unwrap();
    std::fs::write(legacy.join(".claude/.credentials.json"), CREDENTIALS).unwrap();
    legacy
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

#[test]
fn an_existing_legacy_home_is_renamed_to_homes_key_with_its_credentials() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    let legacy = a_legacy_claude_home_with_credentials(tddy_home.path());

    // When
    let home = persistent_home(tddy_home.path(), "claude");

    // Then
    let expected = canonical(tddy_home.path()).join("homes/claude");
    assert_eq!(home, Ok(expected.clone()));
    assert_eq!(
        std::fs::read_to_string(expected.join(".claude/.credentials.json")).unwrap(),
        CREDENTIALS
    );
    assert!(!legacy.exists(), "the legacy home is moved, not copied");
}

#[test]
fn resolving_a_migrated_home_again_returns_it_unchanged() {
    // Given — a home the first resolution migrated
    let tddy_home = tempfile::tempdir().unwrap();
    a_legacy_claude_home_with_credentials(tddy_home.path());
    let first = persistent_home(tddy_home.path(), "claude").expect("the first resolution");

    // When
    let second = persistent_home(tddy_home.path(), "claude");

    // Then
    assert_eq!(second, Ok(first.clone()));
    assert_eq!(
        std::fs::read_to_string(first.join(".claude/.credentials.json")).unwrap(),
        CREDENTIALS
    );
}

#[test]
fn a_home_is_created_empty_when_no_legacy_home_exists() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();

    // When
    let home = persistent_home(tddy_home.path(), "cursor");

    // Then
    let expected = canonical(tddy_home.path()).join("homes/cursor");
    assert_eq!(home, Ok(expected.clone()));
    assert_eq!(std::fs::read_dir(&expected).unwrap().count(), 0);
}

#[test]
fn a_legacy_and_a_new_home_both_existing_is_refused_naming_both_and_touching_neither() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    let legacy = a_legacy_claude_home_with_credentials(tddy_home.path());
    let new_home = tddy_home.path().join("homes/claude");
    std::fs::create_dir_all(&new_home).unwrap();

    // When
    let refusal = persistent_home(tddy_home.path(), "claude");

    // Then
    assert_eq!(
        refusal,
        Err(HomeError::BothExist {
            legacy: legacy.clone(),
            home: new_home.clone(),
        })
    );
    assert_eq!(
        std::fs::read_to_string(legacy.join(".claude/.credentials.json")).unwrap(),
        CREDENTIALS
    );
    assert_eq!(std::fs::read_dir(&new_home).unwrap().count(), 0);
}
