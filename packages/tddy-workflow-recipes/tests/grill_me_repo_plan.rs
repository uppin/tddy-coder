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
    let root = repo("workspace/my-app");
    let got = persisted_grill_me_brief_path(&root, "feature-grill-brief").unwrap();
    let expected = root.join("plans").join("feature-grill-brief.md");
    assert_eq!(got, expected);
}

#[test]
fn accepts_single_segment_alphanumeric_stem() {
    let root = repo("r");
    let got = persisted_grill_me_brief_path(&root, "x").unwrap();
    assert_eq!(got, root.join("plans").join("x.md"));
}

#[test]
fn accepts_stem_with_hyphens() {
    let root = repo("repo");
    let got = persisted_grill_me_brief_path(&root, "a-b-c").unwrap();
    assert_eq!(got, root.join("plans").join("a-b-c.md"));
}

#[test]
fn rejects_empty_plan_stem() {
    let root = repo("repo");
    let err = persisted_grill_me_brief_path(&root, "").unwrap_err();
    assert_eq!(err, GrillMePersistedBriefPathError::EmptyPlanStem);
}

#[test]
fn rejects_whitespace_only_plan_stem() {
    let root = repo("repo");
    let err = persisted_grill_me_brief_path(&root, "   ").unwrap_err();
    assert_eq!(err, GrillMePersistedBriefPathError::EmptyPlanStem);
}

#[test]
fn rejects_plan_stem_with_slash() {
    let root = repo("repo");
    let err = persisted_grill_me_brief_path(&root, "a/b").unwrap_err();
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}

#[test]
fn rejects_plan_stem_with_backslash() {
    let root = repo("repo");
    let err = persisted_grill_me_brief_path(&root, "a\\b").unwrap_err();
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}

#[test]
fn rejects_plan_stem_dot_dot() {
    let root = repo("repo");
    let err = persisted_grill_me_brief_path(&root, "..").unwrap_err();
    assert_eq!(err, GrillMePersistedBriefPathError::InvalidPlanStem);
}
