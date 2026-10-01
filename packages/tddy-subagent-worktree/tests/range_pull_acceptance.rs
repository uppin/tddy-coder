//! Handing a chosen range of a conversation's commits to the caller.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § `subagent_pull` — take part of the work now

mod support;

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use support::{a_caller_worktree, conversation, rev_parse, short_head, write, CallerWorktree};
use tddy_subagent_worktree::{ConversationWorktree, PullRange};

/// A conversation worktree with one commit per `(path, content)`, in order.
async fn a_conversation_that_committed(
    caller: &CallerWorktree,
    writes: &[(&str, &str)],
) -> (ConversationWorktree, Vec<String>) {
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    let mut commits = Vec::new();
    for (path, content) in writes {
        write(worktree.root(), path, content.as_bytes());
        worktree.commit_changes("Write").await.expect("commit");
        commits.push(short_head(worktree.root()));
    }
    (worktree, commits)
}

fn nothing_pulled() -> BTreeSet<String> {
    BTreeSet::new()
}

fn pulled(commits: &[&String]) -> BTreeSet<String> {
    commits.iter().map(|c| c.to_string()).collect()
}

fn range(from: Option<&String>, to: Option<&String>) -> PullRange {
    PullRange {
        from: from.cloned(),
        to: to.cloned(),
    }
}

#[tokio::test]
async fn with_no_bounds_every_unpulled_commit_is_applied_in_order() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;

    // When
    let outcome = worktree
        .pull_range(&PullRange::default(), &nothing_pulled())
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (outcome.commits, caller.read("a.txt"), caller.read("b.txt")),
        (commits, "a\n".to_string(), "b\n".to_string())
    );
}

#[tokio::test]
async fn from_and_to_are_both_inclusive() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_committed(
        &caller,
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
    )
    .await;

    // When
    let outcome = worktree
        .pull_range(
            &range(Some(&commits[1]), Some(&commits[2])),
            &nothing_pulled(),
        )
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (
            outcome.commits,
            caller.exists("a.txt"),
            caller.exists("b.txt"),
            caller.exists("c.txt")
        ),
        (
            vec![commits[1].clone(), commits[2].clone()],
            false,
            true,
            true
        )
    );
}

#[tokio::test]
async fn a_commit_already_pulled_is_skipped_and_reported() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;

    // When
    let outcome = worktree
        .pull_range(
            &range(Some(&commits[0]), Some(&commits[1])),
            &pulled(&[&commits[0]]),
        )
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (outcome.commits, outcome.skipped, caller.exists("a.txt")),
        (vec![commits[1].clone()], vec![commits[0].clone()], false)
    );
}

#[tokio::test]
async fn the_default_lower_bound_is_the_first_commit_not_yet_pulled() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_committed(
        &caller,
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
    )
    .await;

    // When
    let outcome = worktree
        .pull_range(&PullRange::default(), &pulled(&[&commits[0]]))
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (outcome.commits, outcome.skipped),
        (
            vec![commits[1].clone(), commits[2].clone()],
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn a_range_with_nothing_left_to_pull_applies_nothing() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;

    // When
    let outcome = worktree
        .pull_range(&PullRange::default(), &pulled(&[&commits[0]]))
        .await
        .expect("an empty remainder is not an error");

    // Then
    assert_eq!(
        (outcome.commits, caller.exists("a.txt")),
        (Vec::<String>::new(), false)
    );
}

#[tokio::test]
async fn each_commit_is_applied_three_way_and_conflicts_are_named() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\nthree\n")
        .build();
    let (worktree, _) = a_conversation_that_committed(
        &caller,
        &[("src/lib.rs", "one\nsubagent\nthree\n"), ("b.txt", "b\n")],
    )
    .await;
    caller.write("src/lib.rs", "one\ncaller\nthree\n");

    // When
    let outcome = worktree
        .pull_range(&PullRange::default(), &nothing_pulled())
        .await
        .expect("pull");

    // Then — the marker labels vary by git version, so the file is checked for a marker
    assert_eq!(
        (
            outcome.conflicts,
            caller.read("src/lib.rs").contains("<<<<<<<"),
            caller.read("b.txt")
        ),
        (vec!["src/lib.rs".to_string()], true, "b\n".to_string())
    );
}

#[tokio::test]
async fn a_commit_not_on_the_branch_is_refused() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, _) = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;

    // When
    let refused = worktree
        .pull_range(
            &range(Some(&"0000000".to_string()), None),
            &nothing_pulled(),
        )
        .await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_from_after_to_is_refused() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;

    // When
    let refused = worktree
        .pull_range(
            &range(Some(&commits[1]), Some(&commits[0])),
            &nothing_pulled(),
        )
        .await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn pulling_leaves_the_conversation_branch_and_worktree_untouched() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, _) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;
    let tip = rev_parse(worktree.root(), "HEAD");

    // When
    worktree
        .pull_range(&PullRange::default(), &nothing_pulled())
        .await
        .expect("pull");

    // Then
    assert_eq!(
        (
            rev_parse(worktree.root(), "HEAD"),
            worktree.root().join("a.txt").exists()
        ),
        (tip, true)
    );
}
