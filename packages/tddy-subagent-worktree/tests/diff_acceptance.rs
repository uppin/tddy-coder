//! Reading the diff between two points of a conversation.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-diff.md

mod support;

use pretty_assertions::assert_eq;
use support::{a_caller_worktree, conversation, git, short_head, write, CallerWorktree};
use tddy_subagent_worktree::{ConversationWorktree, FileCounts, LineCounts, DIFF_TEXT_CAP_BYTES};

/// A conversation worktree with one commit per `(path, content)`, in order; returns the worktree
/// and each commit's short hash.
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

fn base_short(worktree: &ConversationWorktree) -> String {
    git(worktree.root(), &["rev-parse", "--short", worktree.base()])
        .trim()
        .to_string()
}

#[tokio::test]
async fn with_no_bounds_the_diff_runs_from_the_base_to_the_tip() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;

    // When
    let diff = worktree.diff(None, None).await.expect("diff");

    // Then
    assert_eq!(
        (
            diff.from.clone(),
            diff.to.clone(),
            diff.files,
            diff.truncated
        ),
        (
            base_short(&worktree),
            commits[1].clone(),
            FileCounts {
                created: 2,
                updated: 0,
                removed: 0
            },
            false
        )
    );
}

#[tokio::test]
async fn from_and_to_select_the_changes_after_from_through_to() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) = a_conversation_that_committed(
        &caller,
        &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")],
    )
    .await;

    // When
    let diff = worktree
        .diff(Some(&commits[0]), Some(&commits[1]))
        .await
        .expect("diff");

    // Then
    assert_eq!(
        (
            diff.diff.contains("b.txt"),
            diff.diff.contains("a.txt"),
            diff.diff.contains("c.txt")
        ),
        (true, false, false)
    );
}

#[tokio::test]
async fn the_diff_text_is_what_git_itself_reports_for_the_range() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\n")
        .build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("src/lib.rs", "one\nTWO\n")]).await;

    // When
    let diff = worktree.diff(None, None).await.expect("diff");

    // Then
    assert_eq!(
        diff.diff,
        git(
            worktree.root(),
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                worktree.base(),
                &commits[0]
            ]
        )
    );
}

#[tokio::test]
async fn the_counts_describe_the_whole_range() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\nthree\n")
        .with_committed_file("src/old.rs", "x\n")
        .build();
    let (worktree, _) = a_conversation_that_committed(
        &caller,
        &[
            ("src/lib.rs", "one\nTWO\nthree\nfour\n"),
            ("src/new.rs", "n\n"),
        ],
    )
    .await;
    std::fs::remove_file(worktree.root().join("src/old.rs")).unwrap();
    worktree.commit_changes("Delete").await.expect("commit");

    // When
    let diff = worktree.diff(None, None).await.expect("diff");

    // Then
    assert_eq!(
        (diff.files, diff.lines),
        (
            FileCounts {
                created: 1,
                updated: 1,
                removed: 1
            },
            LineCounts {
                added: 3,
                removed: 2
            }
        )
    );
}

#[tokio::test]
async fn a_diff_past_the_cap_is_cut_at_a_line_and_marked_truncated_with_whole_range_counts() {
    // Given — 20 000 lines of 10 bytes: well past the cap
    let big: String = (0..20_000).map(|i| format!("line {i:04}\n")).collect();
    let caller = a_caller_worktree().build();
    let (worktree, _) = a_conversation_that_committed(&caller, &[("big.txt", &big)]).await;

    // When
    let diff = worktree.diff(None, None).await.expect("diff");

    // Then
    assert_eq!(
        (
            diff.truncated,
            diff.diff.len() <= DIFF_TEXT_CAP_BYTES,
            diff.diff.ends_with('\n'),
            diff.lines,
        ),
        (
            true,
            true,
            true,
            LineCounts {
                added: 20_000,
                removed: 0
            }
        )
    );
}

#[tokio::test]
async fn a_commit_outside_the_conversation_is_refused() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, _) = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
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
    let refused = worktree.diff(Some(outside.trim()), None).await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_commit_dropped_from_the_branch_is_refused() {
    // Given — the second commit is dropped with plain git, not through a reset of this crate's
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;
    git(worktree.root(), &["reset", "-q", "--hard", &commits[0]]);

    // When
    let refused = worktree.diff(None, Some(&commits[1])).await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_from_that_is_not_an_ancestor_of_to_is_refused() {
    // Given
    let caller = a_caller_worktree().build();
    let (worktree, commits) =
        a_conversation_that_committed(&caller, &[("a.txt", "a\n"), ("b.txt", "b\n")]).await;

    // When
    let refused = worktree.diff(Some(&commits[1]), Some(&commits[0])).await;

    // Then
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_binary_change_shows_as_binary() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(
        worktree.root(),
        "logo.png",
        &[0x89, b'P', b'N', b'G', 0x00, 0x01],
    );
    worktree.commit_changes("Write").await.expect("commit");

    // When
    let diff = worktree.diff(None, None).await.expect("diff");

    // Then — git's own sentence for a binary file, whatever the version's exact path rendering
    assert!(diff.diff.contains("Binary files"));
}
