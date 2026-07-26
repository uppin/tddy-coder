//! Acceptance: which checkout a session directory names as its repo root.
//!
//! Two files in a session directory can record it, and they are written at different moments:
//!
//! - `.session.yaml` — written at session start for **every** session, naming the checkout the
//!   session was started over,
//! - `changeset.yaml` — gains `repo_path` only when the session is given a worktree of its own.
//!
//! A pr-stack orchestrator is a planning session that never creates a worktree, so its changeset
//! records no repo at all. A caller that reads only the changeset therefore learns nothing about a
//! perfectly well-known checkout — and silently substituting the session directory points git at a
//! path that is not a repository, which is how a merged PR came to read as "no PR exists" on its
//! planned-PR row.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D8).

use std::path::{Path, PathBuf};

use tddy_core::repo_root_for_session;
use tddy_testing_commons::{a_changeset, a_session_metadata, fs::write_session_yaml};

/// The main checkout a pr-stack orchestrator plans over.
const MAIN_CHECKOUT: &str = "/var/tddy/Code/tddy-coder";
/// The worktree a child session of that stack works in.
const CHILD_WORKTREE: &str =
    "/var/tddy/Code/tddy-coder/.worktrees/feature-attach-docs-attach-proto";

const SESSION_ID: &str = "019f9dd5-716d-7071-96ac-464ff7b98c2a";

/// A fresh, empty session directory — neither file written yet.
fn a_session_dir(temp: &tempfile::TempDir) -> PathBuf {
    let dir = temp.path().join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&dir).expect("create session dir in test setup");
    dir
}

/// `changeset.yaml` naming the checkout — the shape a session with its own worktree has.
fn a_changeset_recording_the_repo(session_dir: &Path, repo_path: &str) {
    let changeset = a_changeset().with_repo_path(repo_path).build();
    tddy_core::write_changeset(session_dir, &changeset).expect("write changeset in test setup");
}

/// `changeset.yaml` for a pr-stack orchestrator: it carries a recipe and no repo path at all.
fn a_changeset_recording_no_repo(session_dir: &Path) {
    let changeset = a_changeset().with_recipe("pr-stack").build();
    tddy_core::write_changeset(session_dir, &changeset).expect("write changeset in test setup");
}

/// `.session.yaml` naming the checkout the session was started over.
fn session_metadata_recording_the_repo(session_dir: &Path, repo_path: &str) {
    write_session_yaml(
        session_dir,
        &a_session_metadata()
            .with_session_id(SESSION_ID)
            .with_repo_path(repo_path)
            .build(),
    );
}

/// `.session.yaml` with no repo path — a legacy file, written before the field existed.
fn session_metadata_recording_no_repo(session_dir: &Path) {
    write_session_yaml(
        session_dir,
        &a_session_metadata().with_session_id(SESSION_ID).build(),
    );
}

#[test]
fn prefers_the_repo_path_recorded_in_the_changeset() {
    // Given — a child session working in its own worktree, started over the main checkout
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(&temp);
    a_changeset_recording_the_repo(&session_dir, CHILD_WORKTREE);
    session_metadata_recording_the_repo(&session_dir, MAIN_CHECKOUT);

    // When
    let repo_root = repo_root_for_session(&session_dir);

    // Then — the worktree the session actually works in, not the checkout it was started from
    assert_eq!(repo_root, Some(PathBuf::from(CHILD_WORKTREE)));
}

#[test]
fn falls_back_to_the_session_metadata_repo_path_when_the_changeset_records_none() {
    // Given — a pr-stack orchestrator: a recipe, no worktree of its own, so no repo in its changeset
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(&temp);
    a_changeset_recording_no_repo(&session_dir);
    session_metadata_recording_the_repo(&session_dir, MAIN_CHECKOUT);

    // When
    let repo_root = repo_root_for_session(&session_dir);

    // Then — the checkout it plans over is known, and must not degrade to the session directory
    assert_eq!(repo_root, Some(PathBuf::from(MAIN_CHECKOUT)));
}

#[test]
fn resolves_the_repo_path_from_the_session_metadata_when_the_session_has_no_changeset_file() {
    // Given — a session that has started but written no changeset yet
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(&temp);
    session_metadata_recording_the_repo(&session_dir, MAIN_CHECKOUT);

    // When
    let repo_root = repo_root_for_session(&session_dir);

    // Then — an unreadable changeset is not an answer about the repo, so metadata still decides
    assert_eq!(repo_root, Some(PathBuf::from(MAIN_CHECKOUT)));
}

#[test]
fn resolves_no_repo_root_when_neither_the_changeset_nor_the_session_metadata_records_one() {
    // Given — a legacy session directory that names no checkout anywhere
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(&temp);
    a_changeset_recording_no_repo(&session_dir);
    session_metadata_recording_no_repo(&session_dir);

    // When
    let repo_root = repo_root_for_session(&session_dir);

    // Then — callers must learn the repo is unknown, never be handed the session directory
    assert_eq!(
        repo_root, None,
        "an unknown repo root must be reported as unknown, not substituted"
    );
}
