//! Acceptance: which branch a spawn is keyed on when linking it to a PR-stack node.
//!
//! `resolve_chain_base_ref` and `link_stack_node_to_spawned_branch` are both keyed on
//! `new_branch_name`, which is **empty** in `work_on_selected_branch` mode. `pr_stack_node_for_spawn`
//! returns `None` for a blank branch, so resuming an existing branch never re-links the node: the row
//! would stay in its recovered state and every click would spawn another unlinked session.
//!
//! Recovering a planned PR whose session was deleted means resuming the branch the node already owns
//! (it exists, is pushed, and has a worktree), so the link has to follow the branch the spawn actually
//! operates on — whichever intent produced it.
//!
//! And which branch is *recorded*, which is not always the one that was asked for:
//! `create_worktree_with_retry` appends `-1`, `-2`, … on a name collision — the default
//! `on_branch_conflict` behaviour — and writes the real name into the session's changeset. A node
//! recording the requested name advertises a branch nobody has, so every descendant bases onto a
//! `<remote>/<branch>` that does not exist and the cross-host row names a branch no host resolves.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C1, D2, D3, D34).

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_daemon::connection_service::{effective_spawn_branch, spawned_branch_of_session};

#[test]
fn a_new_branch_spawn_is_keyed_on_the_branch_it_creates() {
    // Given / When
    let branch = effective_spawn_branch(
        "new_branch_from_base",
        "feature/attach-docs/attach-start",
        "",
        "origin",
    );

    // Then
    assert_eq!(branch, "feature/attach-docs/attach-start");
}

#[test]
fn an_existing_branch_spawn_is_keyed_on_the_branch_it_resumes() {
    // Given — resuming the branch an orphaned node already owns; no new branch is created
    // When
    let branch = effective_spawn_branch(
        "work_on_selected_branch",
        "",
        "feature/attach-docs/attach-store",
        "origin",
    );

    // Then — this is what lets the resumed session re-link to its planned node
    assert_eq!(branch, "feature/attach-docs/attach-store");
}

#[test]
fn an_unset_intent_is_keyed_on_the_new_branch_name() {
    // Given — an empty intent defaults to new_branch_from_base (StartSessionRequest field 9)
    // When
    let branch = effective_spawn_branch("", "feature/attach-docs/attach-start", "", "origin");

    // Then
    assert_eq!(branch, "feature/attach-docs/attach-start");
}

#[test]
fn a_new_branch_spawn_ignores_a_selected_branch_the_dialog_sent_alongside_it() {
    // Given — `CreateSessionPane` submits `selectedBranchToWorkOn` on every request, whatever the
    // chosen mode, so a new-branch spawn routinely carries the picker's current selection
    // When
    let branch = effective_spawn_branch(
        "new_branch_from_base",
        "feature/attach-docs/attach-start",
        "origin/master",
        "origin",
    );

    // Then — keying on whichever field happens to be non-empty would link the node to `master`
    assert_eq!(branch, "feature/attach-docs/attach-start");
}

#[test]
fn an_existing_branch_spawn_is_keyed_on_the_local_branch_behind_a_remote_tracking_name() {
    // Given — the dialog's branch picker is fed by `ListProjectBranches`, which offers remote-tracking
    // names (`list_recent_remote_branches` reads `refs/remotes/<remote>`), so this is the shape the
    // daemon actually receives from the web
    // When
    let branch = effective_spawn_branch(
        "work_on_selected_branch",
        "",
        "origin/feature/attach-docs/attach-store",
        "origin",
    );

    // Then — a stack node records the *local* branch, so keying on the prefixed name matches no node:
    // the resumed session would stay unlinked and every click would spawn another orphan
    assert_eq!(branch, "feature/attach-docs/attach-store");
}

#[test]
fn an_existing_branch_spawn_ignores_a_stale_new_branch_name() {
    // Given — the dialog carries a leftover new-branch value while the intent says resume
    // When
    let branch = effective_spawn_branch(
        "work_on_selected_branch",
        "feature/stale-leftover",
        "feature/attach-docs/attach-store",
        "origin",
    );

    // Then — keying on the leftover would link the node to a branch the spawn never touches
    assert_eq!(branch, "feature/attach-docs/attach-store");
}

#[test]
fn an_existing_branch_spawn_strips_a_non_origin_remote_prefix() {
    // Given — a project whose default remote is `upstream`; the picker offers `upstream/<branch>`
    // names, so the daemon receives the value with that prefix
    // When
    let branch = effective_spawn_branch(
        "work_on_selected_branch",
        "",
        "upstream/feature/attach-docs/attach-store",
        "upstream",
    );

    // Then — the node is keyed on the local branch regardless of which remote the picker used
    assert_eq!(branch, "feature/attach-docs/attach-store");
}

/// A session directory whose worktree setup recorded `branch` — what
/// `setup_worktree_for_session_with_optional_chain_base` writes once the branch exists.
fn a_session_that_created(branch: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp session dir");
    write_changeset(
        dir.path(),
        &Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    )
    .expect("write the session's changeset");
    dir
}

#[test]
fn a_collision_suffixed_branch_is_the_one_the_link_records() {
    // Given — the spawn asked for `feature/attach-docs/attach-store`, and the retry loop landed on
    // `-1` because a branch of that name already existed
    let session = a_session_that_created("feature/attach-docs/attach-store-1");

    // When
    let branch = spawned_branch_of_session(session.path(), "feature/attach-docs/attach-store");

    // Then — recording the requested name would point every descendant at a ref nobody created
    assert_eq!(branch, "feature/attach-docs/attach-store-1");
}

#[test]
fn a_spawn_whose_session_recorded_no_branch_is_keyed_on_the_branch_it_asked_for() {
    // Given — a client-supplied `repo_path` spawn: no worktree is created, so no branch is written
    let session = tempfile::tempdir().expect("temp session dir");

    // When
    let branch = spawned_branch_of_session(session.path(), "feature/attach-docs/attach-store");

    // Then
    assert_eq!(branch, "feature/attach-docs/attach-store");
}
