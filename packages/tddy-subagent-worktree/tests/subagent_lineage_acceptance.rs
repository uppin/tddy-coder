//! After a sync, the caller's merged changes are never the subagent's work: pulls, resets and diffs
//! read the branch's first-parent line without merges.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md

mod support;

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use support::{a_caller_worktree, conversation, rev_parse, short_head, write, CallerWorktree};
use tddy_subagent_worktree::{ConversationWorktree, PullRange, ResetTarget, SyncOutcome};

/// A conversation that committed `a.txt` (c1), took in a caller edit of `caller.txt` (a sync merge),
/// then committed `b.txt` (c2).
struct ASyncedConversation {
    caller: CallerWorktree,
    worktree: ConversationWorktree,
    c1: String,
    merge: String,
    c2: String,
}

async fn a_conversation_with_a_sync_between_two_commits() -> ASyncedConversation {
    let caller = a_caller_worktree()
        .with_committed_file("caller.txt", "before\n")
        .build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "a.txt", b"a\n");
    worktree.commit_changes("Write").await.expect("c1");
    let c1 = short_head(worktree.root());
    caller.write("caller.txt", "after\n");
    let merge = match worktree.sync_with_caller().await.expect("sync") {
        SyncOutcome::Merged(sync) => sync.commit,
        other => panic!("the fixture needs a merge, got {other:?}"),
    };
    write(worktree.root(), "b.txt", b"b\n");
    worktree.commit_changes("Write").await.expect("c2");
    let c2 = short_head(worktree.root());
    ASyncedConversation {
        caller,
        worktree,
        c1,
        merge,
        c2,
    }
}

fn full(worktree: &ConversationWorktree, short: &str) -> String {
    rev_parse(worktree.root(), short)
}

#[tokio::test]
async fn the_subagents_commits_leave_out_the_merge_and_the_callers_snapshot() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let commits = synced.worktree.subagent_commits().await.expect("list");

    // Then
    assert_eq!(
        commits,
        vec![
            full(&synced.worktree, &synced.c1),
            full(&synced.worktree, &synced.c2)
        ]
    );
}

#[tokio::test]
async fn a_range_pull_after_a_sync_applies_only_the_subagents_commits() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let pulled = synced
        .worktree
        .pull_range(&PullRange::default(), &BTreeSet::new())
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (
            pulled.commits,
            pulled.conflicts,
            synced.caller.read("caller.txt")
        ),
        (
            vec![synced.c1.clone(), synced.c2.clone()],
            Vec::<String>::new(),
            "after\n".to_string()
        )
    );
}

#[tokio::test]
async fn pull_into_caller_never_hands_the_caller_its_own_changes_back() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let pulled = synced.worktree.pull_into_caller().await.expect("pull");

    // Then
    assert_eq!(
        (
            pulled.conflicts,
            pulled.files.created,
            synced.caller.read("caller.txt")
        ),
        (Vec::<String>::new(), 2, "after\n".to_string())
    );
}

#[tokio::test]
async fn a_reset_never_lists_the_merge_as_dropped() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let reset = synced
        .worktree
        .reset_to(&ResetTarget::Commit(synced.c1.clone()))
        .await
        .expect("reset");

    // Then
    assert_eq!(reset.dropped_commits, vec![synced.c2.clone()]);
}

#[tokio::test]
async fn a_reset_past_a_sync_drops_it_and_the_next_sync_takes_the_caller_in_again() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;
    synced
        .worktree
        .reset_to(&ResetTarget::Commit(synced.c1.clone()))
        .await
        .expect("reset");

    // When
    let outcome = synced.worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (
            matches!(outcome, SyncOutcome::Merged(_)),
            std::fs::read_to_string(synced.worktree.root().join("caller.txt")).unwrap()
        ),
        (true, "after\n".to_string())
    );
}

#[tokio::test]
async fn a_merge_commit_is_refused_as_a_reset_target() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;
    let tip = rev_parse(synced.worktree.root(), "HEAD");

    // When
    let refused = synced
        .worktree
        .reset_to(&ResetTarget::Commit(synced.merge.clone()))
        .await;

    // Then
    assert_eq!(
        (refused.is_err(), rev_parse(synced.worktree.root(), "HEAD")),
        (true, tip)
    );
}

#[tokio::test]
async fn a_merge_commit_is_refused_as_a_diff_bound() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let refused = synced.worktree.diff(Some(&synced.merge), None).await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_diff_spanning_a_sync_says_it_includes_caller_changes() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let diff = synced
        .worktree
        .diff(Some(&synced.c1), Some(&synced.c2))
        .await
        .expect("diff");

    // Then
    assert!(diff.includes_caller_changes);
}

/// Guards the plain case: a range with no sync in it is the subagent's alone.
#[tokio::test]
async fn a_diff_with_no_sync_in_its_range_does_not() {
    // Given
    let synced = a_conversation_with_a_sync_between_two_commits().await;

    // When
    let diff = synced
        .worktree
        .diff(None, Some(&synced.c1))
        .await
        .expect("diff");

    // Then
    assert!(!diff.includes_caller_changes);
}
