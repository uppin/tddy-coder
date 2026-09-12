use super::*;
use std::time::Duration;

/// A session dir whose changeset records `branch` but has not been pushed.
fn a_session_with_unpushed_branch() -> tempfile::TempDir {
    let session = tempfile::tempdir().unwrap();
    let cs = tddy_core::changeset::Changeset {
        branch: Some("feature/x".to_string()),
        remote_pushed: false,
        ..Default::default()
    };
    tddy_core::write_changeset(session.path(), &cs).unwrap();
    session
}

/// The push is skipped for "work on existing branch" even when Create Remote Branch is ticked —
/// only a freshly created branch is ever pushed. The worktree is not a git repo, so a broken
/// guard that attempted the push would fail loudly instead of returning Ok.
#[tokio::test]
async fn push_is_skipped_for_work_on_selected_branch_intent() {
    // Given
    let session = a_session_with_unpushed_branch();
    let worktree = tempfile::tempdir().unwrap();

    // When
    let result = push_new_branch_to_origin_if_requested(
        true,
        tddy_core::changeset::BranchWorktreeIntent::WorkOnSelectedBranch,
        session.path(),
        worktree.path(),
        Duration::from_secs(10),
    )
    .await;

    // Then
    assert!(
        result.is_ok(),
        "gated call must succeed without pushing: {result:?}"
    );
    let after = tddy_core::read_changeset(session.path()).unwrap();
    assert!(
        !after.remote_pushed,
        "remote_pushed must stay false when intent is not new-branch"
    );
}

/// The push is skipped when the operator opts out (flag false), even for a new branch.
#[tokio::test]
async fn push_is_skipped_when_create_remote_branch_is_false() {
    // Given
    let session = a_session_with_unpushed_branch();
    let worktree = tempfile::tempdir().unwrap();

    // When
    let result = push_new_branch_to_origin_if_requested(
        false,
        tddy_core::changeset::BranchWorktreeIntent::NewBranchFromBase,
        session.path(),
        worktree.path(),
        Duration::from_secs(10),
    )
    .await;

    // Then
    assert!(
        result.is_ok(),
        "opt-out call must succeed without pushing: {result:?}"
    );
    let after = tddy_core::read_changeset(session.path()).unwrap();
    assert!(
        !after.remote_pushed,
        "remote_pushed must stay false when the flag is off"
    );
}
