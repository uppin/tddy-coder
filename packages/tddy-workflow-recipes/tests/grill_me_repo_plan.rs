//! Acceptance: default repo path for a persisted grill-me brief (`plans/<stem>.md`), per AGENTS.md.

use std::path::PathBuf;

use tddy_workflow_recipes::grill_me::{
    persisted_grill_me_brief_path, GrillMePersistedBriefPathError,
};

fn repo(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[test]
fn joins_repo_root_plans_directory_and_stem_with_md_extension() {
    // Given
    let root = repo("workspace/my-app");

    // When
    let got = persisted_grill_me_brief_path(&root, "feature-grill-brief").unwrap();

    // Then
    let expected = root.join("plans").join("feature-grill-brief.md");
    assert_eq!(got, expected);
}

#[test]
fn accepts_single_segment_alphanumeric_stem() {
    // Given
    let root = repo("r");

    // When
    let got = persisted_grill_me_brief_path(&root, "x").unwrap();

    // Then
    assert_eq!(got, root.join("plans").join("x.md"));
}

#[test]
fn accepts_stem_with_hyphens() {
    // Given
    let root = repo("repo");

    // When
    let got = persisted_grill_me_brief_path(&root, "a-b-c").unwrap();

    // Then
    assert_eq!(got, root.join("plans").join("a-b-c.md"));
}

#[test]
fn rejects_empty_plan_stem() {
    // Given
    let root = repo("repo");

    // When
    let err = persisted_grill_me_brief_path(&root, "").unwrap_err();

    // Then
    assert_eq!(err, GrillMePersistedBriefPathError::EmptyPlanStem);
}

#[test]
fn rejects_whitespace_only_plan_stem() {
    // Given
    let root = repo("repo");

    // When
    let err = persisted_grill_me_brief_path(&root, "   ").unwrap_err();

    // Then
    assert_eq!(err, GrillMePersistedBriefPathError::EmptyPlanStem);
}

#[test]
fn rejects_plan_stem_with_slash() {
    // Given
    let root = repo("repo");

    // When
    let err = persisted_grill_me_brief_path(&root, "a/b").unwrap_err();

    // Then
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}

#[test]
fn rejects_plan_stem_with_backslash() {
    // Given
    let root = repo("repo");

    // When
    let err = persisted_grill_me_brief_path(&root, "a\\b").unwrap_err();

    // Then
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}

#[test]
fn rejects_plan_stem_dot_dot() {
    // Given
    let root = repo("repo");

    // When
    let err = persisted_grill_me_brief_path(&root, "..").unwrap_err();

    // Then
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}
