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
//! PRD: docs/ft/coder/pr-stack-live-status.md (C1, D2, D3).

use tddy_daemon::connection_service::effective_spawn_branch;

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
