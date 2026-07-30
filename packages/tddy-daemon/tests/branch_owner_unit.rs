//! Unit tests: the one rule for "which session owns a branch", shared by `QueryBranch`, the
//! `StartSession` branch-conflict guard and the Telegram spawn flow.
//!
//! Ownership is claimed by `Changeset.branch` alone. When several sessions claim the same branch an
//! active one wins, and between equally active ones the most recently updated does.
//!
//! PRD: docs/ft/daemon/session-branch-conflict.md

use std::path::Path;

use tddy_core::changeset::Changeset;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::branch_owner::find_session_owning_branch;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const BRANCH: &str = "feat/auth";
const FIRST_SESSION: &str = "019d6392-3cff-0001-aaaa-000000000001";
const SECOND_SESSION: &str = "019d6392-3cff-0001-aaaa-000000000002";

/// A session on `branch`. `alive` makes it read as active — the current process is the only pid
/// guaranteed to be running, and a session with no pid reads as idle.
fn a_session_on(
    sessions_base: &Path,
    session_id: &str,
    branch: &str,
    alive: bool,
    updated_at: &str,
) {
    let dir = a_session_dir(sessions_base, session_id);
    tddy_core::write_changeset(
        &dir,
        &Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    )
    .unwrap();
    let builder = a_session_metadata()
        .with_session_id(session_id)
        .with_status(if alive { "active" } else { "idle" });
    let mut metadata = if alive {
        builder.with_pid(std::process::id()).build()
    } else {
        builder.build()
    };
    metadata.updated_at = updated_at.to_string();
    write_session_yaml(&dir, &metadata);
}

/// A session listed on disk whose `changeset.yaml` is unreadable — it names no branch.
fn a_session_with_an_unreadable_changeset(sessions_base: &Path, session_id: &str) {
    let dir = a_session_dir(sessions_base, session_id);
    std::fs::write(dir.join("changeset.yaml"), "{{{ not yaml").unwrap();
    write_session_yaml(
        &dir,
        &a_session_metadata().with_session_id(session_id).build(),
    );
}

fn a_session_dir(sessions_base: &Path, session_id: &str) -> std::path::PathBuf {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn finds_the_session_whose_changeset_names_the_branch() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_on(
        temp.path(),
        FIRST_SESSION,
        BRANCH,
        false,
        "2026-07-30T09:00:00Z",
    );

    // When
    let owner = find_session_owning_branch(temp.path(), BRANCH).unwrap();

    // Then
    assert_eq!(owner.map(|s| s.session_id), Some(FIRST_SESSION.to_string()));
}

#[test]
fn finds_no_owner_when_no_session_names_the_branch() {
    // Given — a session exists, but on a different branch
    let temp = tempfile::tempdir().unwrap();
    a_session_on(
        temp.path(),
        FIRST_SESSION,
        "feat/other",
        false,
        "2026-07-30T09:00:00Z",
    );

    // When
    let owner = find_session_owning_branch(temp.path(), BRANCH).unwrap();

    // Then — a branch nobody claims has no owner, so nothing is there to switch to
    assert_eq!(owner.map(|s| s.session_id), None);
}

#[test]
fn prefers_the_active_owner_over_a_more_recently_updated_idle_one() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_on(
        temp.path(),
        FIRST_SESSION,
        BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );
    a_session_on(
        temp.path(),
        SECOND_SESSION,
        BRANCH,
        false,
        "2026-07-30T18:00:00Z",
    );

    // When
    let owner = find_session_owning_branch(temp.path(), BRANCH).unwrap();

    // Then — a running agent is the one an operator would switch to, however stale its metadata
    assert_eq!(owner.map(|s| s.session_id), Some(FIRST_SESSION.to_string()));
}

#[test]
fn prefers_the_most_recently_updated_owner_when_neither_is_active() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    a_session_on(
        temp.path(),
        FIRST_SESSION,
        BRANCH,
        false,
        "2026-07-30T09:00:00Z",
    );
    a_session_on(
        temp.path(),
        SECOND_SESSION,
        BRANCH,
        false,
        "2026-07-30T18:00:00Z",
    );

    // When
    let owner = find_session_owning_branch(temp.path(), BRANCH).unwrap();

    // Then
    assert_eq!(
        owner.map(|s| s.session_id),
        Some(SECOND_SESSION.to_string())
    );
}

#[test]
fn skips_a_session_whose_changeset_cannot_be_read() {
    // Given — an unreadable changeset sits ahead of the real owner in directory order
    let temp = tempfile::tempdir().unwrap();
    a_session_with_an_unreadable_changeset(temp.path(), FIRST_SESSION);
    a_session_on(
        temp.path(),
        SECOND_SESSION,
        BRANCH,
        false,
        "2026-07-30T09:00:00Z",
    );

    // When
    let owner = find_session_owning_branch(temp.path(), BRANCH).unwrap();

    // Then — a session that names no branch claims none, and does not abort the scan
    assert_eq!(
        owner.map(|s| s.session_id),
        Some(SECOND_SESSION.to_string())
    );
}
