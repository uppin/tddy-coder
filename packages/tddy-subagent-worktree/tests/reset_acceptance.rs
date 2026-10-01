//! Taking a conversation's worktree back to an earlier commit.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A rewind takes the worktree back

mod support;

use pretty_assertions::assert_eq;
use support::{a_caller_worktree, conversation, git, rev_parse, short_head, write};
use tddy_subagent_worktree::{ConversationWorktree, ResetTarget, WorktreeReset};

/// A conversation worktree holding one commit per entry of `files`, in order; returns the worktree
/// and the short hash of each commit.
async fn a_conversation_that_wrote(
    caller: &support::CallerWorktree,
    files: &[&str],
) -> (ConversationWorktree, Vec<String>) {
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    let mut commits = Vec::new();
    for file in files {
        write(worktree.root(), file, format!("{file}\n").as_bytes());
        worktree.commit_changes("Write").await.expect("commit");
        commits.push(short_head(worktree.root()));
    }
    (worktree, commits)
}

#[tokio::test]
async fn resetting_to_a_commit_drops_the_commits_after_it() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_wrote(&caller, &["a.txt", "b.txt", "c.txt"]).await;

    // When
    let reset = worktree
        .reset_to(&ResetTarget::Commit(commits[0].clone()))
        .await
        .expect("reset");

    // Then
    assert_eq!(
        (
            reset,
            short_head(worktree.root()),
            worktree.root().join("a.txt").exists(),
            worktree.root().join("b.txt").exists(),
        ),
        (
            WorktreeReset {
                to: commits[0].clone(),
                dropped_commits: vec![commits[1].clone(), commits[2].clone()],
            },
            commits[0].clone(),
            true,
            false,
        )
    );
}

#[tokio::test]
async fn resetting_to_the_base_drops_every_subagent_commit() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_wrote(&caller, &["a.txt", "b.txt"]).await;

    // When
    let reset = worktree.reset_to(&ResetTarget::Base).await.expect("reset");

    // Then
    assert_eq!(
        (reset.dropped_commits, rev_parse(worktree.root(), "HEAD")),
        (commits, rev_parse(worktree.root(), worktree.base()))
    );
}

#[tokio::test]
async fn a_reset_removes_untracked_files_but_keeps_ignored_ones() {
    // Given
    let caller = a_caller_worktree()
        .with_ignored_file("target/keep.log", "build output\n")
        .build();
    let (worktree, _) = a_conversation_that_wrote(&caller, &["a.txt"]).await;
    write(worktree.root(), "stray.txt", b"never committed\n");
    write(worktree.root(), "target/keep.log", b"build output\n");

    // When
    worktree.reset_to(&ResetTarget::Base).await.expect("reset");

    // Then
    assert_eq!(
        (
            worktree.root().join("stray.txt").exists(),
            worktree.root().join("target/keep.log").exists()
        ),
        (false, true)
    );
}

#[tokio::test]
async fn resetting_to_the_tip_drops_nothing() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_wrote(&caller, &["a.txt"]).await;

    // When
    let reset = worktree
        .reset_to(&ResetTarget::Commit(commits[0].clone()))
        .await
        .expect("reset");

    // Then
    assert_eq!(reset.dropped_commits, Vec::<String>::new());
}

#[tokio::test]
async fn a_commit_outside_the_conversation_is_refused_and_nothing_moves() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, _) = a_conversation_that_wrote(&caller, &["a.txt"]).await;
    let tip = rev_parse(worktree.root(), "HEAD");
    let outside = git(
        caller.path(),
        &[
            "commit-tree",
            "-m",
            "elsewhere",
            &format!("{}^{{tree}}", caller.head()),
        ],
    );

    // When
    let refused = worktree
        .reset_to(&ResetTarget::Commit(outside.trim().to_string()))
        .await;

    // Then
    assert_eq!(
        (refused.is_err(), rev_parse(worktree.root(), "HEAD")),
        (true, tip)
    );
}
