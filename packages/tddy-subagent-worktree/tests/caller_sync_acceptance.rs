//! Bringing a conversation's worktree up to the caller's current files before a turn.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md

mod support;

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use support::{a_caller_worktree, conversation, git, rev_parse, short_head, write, CallerWorktree};
use tddy_subagent_worktree::{
    ConversationWorktree, FileCounts, LineCounts, PullRange, SyncOutcome, WorktreeSync,
    OUTSIDE_A_TOOL_CALL_SUBJECT, SYNC_NOTICE_PATHS,
};

/// A conversation worktree holding one subagent commit per `(path, content)`, in order.
async fn a_conversation_that_committed(
    caller: &CallerWorktree,
    writes: &[(&str, &str)],
) -> ConversationWorktree {
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    for (path, content) in writes {
        write(worktree.root(), path, content.as_bytes());
        worktree.commit_changes("Write").await.expect("commit");
    }
    worktree
}

fn read_in(worktree: &ConversationWorktree, path: &str) -> String {
    std::fs::read_to_string(worktree.root().join(path)).expect("read conversation file")
}

fn first_parent_subjects(worktree: &ConversationWorktree) -> Vec<String> {
    git(
        worktree.root(),
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%s",
            &format!("{}..HEAD", worktree.base()),
        ],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

fn merged(outcome: SyncOutcome) -> WorktreeSync {
    match outcome {
        SyncOutcome::Merged(sync) => sync,
        other => panic!("expected the caller's changes to be merged, got {other:?}"),
    }
}

#[tokio::test]
async fn a_caller_edit_is_merged_into_the_conversation_worktree() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\n")
        .build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("src/lib.rs", "one\ntwo\ncaller\n");

    // When
    let outcome = worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (
            merged(outcome).paths,
            read_in(&worktree, "src/lib.rs"),
            git(worktree.root(), &["status", "--porcelain"]),
        ),
        (
            vec!["src/lib.rs".to_string()],
            "one\ntwo\ncaller\n".to_string(),
            String::new()
        )
    );
}

#[tokio::test]
async fn the_subagents_unpulled_commits_survive_the_sync() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("subagent.txt", "mine\n")]).await;
    caller.write("caller.txt", "theirs\n");

    // When
    worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (
            read_in(&worktree, "subagent.txt"),
            read_in(&worktree, "caller.txt")
        ),
        ("mine\n".to_string(), "theirs\n".to_string())
    );
}

#[tokio::test]
async fn the_merge_has_the_subagents_tip_as_its_first_parent() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    let tip = rev_parse(worktree.root(), "HEAD");
    caller.write("caller.txt", "theirs\n");

    // When
    let sync = merged(worktree.sync_with_caller().await.expect("sync"));

    // Then
    assert_eq!(
        (
            rev_parse(worktree.root(), "HEAD^1"),
            short_head(worktree.root())
        ),
        (tip, sync.commit)
    );
}

#[tokio::test]
async fn changes_the_caller_already_pulled_merge_without_conflict() {
    // Given — the caller took the subagent's edit, then changed another line of the same file
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\nthree\n")
        .build();
    let worktree =
        a_conversation_that_committed(&caller, &[("src/lib.rs", "ONE\ntwo\nthree\n")]).await;
    worktree
        .pull_range(&PullRange::default(), &BTreeSet::new())
        .await
        .expect("pull");
    caller.write("src/lib.rs", "ONE\ntwo\nTHREE\n");

    // When
    let outcome = worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (merged(outcome).paths, read_in(&worktree, "src/lib.rs")),
        (
            vec!["src/lib.rs".to_string()],
            "ONE\ntwo\nTHREE\n".to_string()
        )
    );
}

#[tokio::test]
async fn a_caller_that_has_not_changed_merges_nothing() {
    // Given
    let caller = a_caller_worktree()
        .with_untracked_file("notes.md", "draft\n")
        .build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    let tip = rev_parse(worktree.root(), "HEAD");

    // When
    let outcome = worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (outcome, rev_parse(worktree.root(), "HEAD")),
        (SyncOutcome::Unchanged, tip)
    );
}

#[tokio::test]
async fn a_second_sync_with_nothing_new_from_the_caller_merges_nothing() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("caller.txt", "theirs\n");
    worktree.sync_with_caller().await.expect("first sync");
    let tip = rev_parse(worktree.root(), "HEAD");

    // When
    let outcome = worktree.sync_with_caller().await.expect("second sync");

    // Then
    assert_eq!(
        (outcome, rev_parse(worktree.root(), "HEAD")),
        (SyncOutcome::Unchanged, tip)
    );
}

#[tokio::test]
async fn a_sync_merges_against_the_last_caller_state_taken_in() {
    // Given — the caller rewrites the same line twice, with a sync in between; the subagent never
    // touched it. Against the conversation's start this would conflict; against the last sync it is
    // the caller's change alone.
    let caller = a_caller_worktree()
        .with_committed_file("README.md", "hello\n")
        .build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("README.md", "v1\n");
    worktree.sync_with_caller().await.expect("first sync");
    caller.write("README.md", "v2\n");

    // When
    let outcome = worktree.sync_with_caller().await.expect("second sync");

    // Then
    assert_eq!(
        (merged(outcome).paths, read_in(&worktree, "README.md")),
        (vec!["README.md".to_string()], "v2\n".to_string())
    );
}

#[tokio::test]
async fn a_commit_the_caller_made_since_the_start_is_merged() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("committed.txt", "c\n");
    git(caller.path(), &["add", "committed.txt"]);
    git(caller.path(), &["commit", "-q", "-m", "caller commit"]);

    // When
    worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(read_in(&worktree, "committed.txt"), "c\n".to_string());
}

#[tokio::test]
async fn a_conflict_merges_nothing_and_names_the_paths() {
    // Given — the subagent's unpulled commit and the caller both rewrote line two
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\nthree\n")
        .build();
    let worktree =
        a_conversation_that_committed(&caller, &[("src/lib.rs", "one\nsubagent\nthree\n")]).await;
    let tip = rev_parse(worktree.root(), "HEAD");
    caller.write("src/lib.rs", "one\ncaller\nthree\n");

    // When
    let outcome = worktree.sync_with_caller().await.expect("sync answers");

    // Then
    assert_eq!(
        (
            outcome,
            rev_parse(worktree.root(), "HEAD"),
            read_in(&worktree, "src/lib.rs")
        ),
        (
            SyncOutcome::Conflicted {
                paths: vec!["src/lib.rs".to_string()]
            },
            tip,
            "one\nsubagent\nthree\n".to_string()
        )
    );
}

#[tokio::test]
async fn the_sync_reports_what_the_merge_changed() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\n")
        .with_committed_file("src/old.rs", "x\n")
        .build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("src/lib.rs", "one\nTWO\nthree\n");
    caller.write("src/new.rs", "n\n");
    std::fs::remove_file(caller.path().join("src/old.rs")).unwrap();

    // When
    let sync = merged(worktree.sync_with_caller().await.expect("sync"));

    // Then
    assert_eq!(
        (sync.files, sync.lines, sync.paths, sync.more_paths),
        (
            FileCounts {
                created: 1,
                updated: 1,
                removed: 1
            },
            LineCounts {
                added: 3,
                removed: 2
            },
            vec![
                "src/lib.rs".to_string(),
                "src/new.rs".to_string(),
                "src/old.rs".to_string()
            ],
            0
        )
    );
}

#[tokio::test]
async fn paths_past_the_limit_are_counted_not_listed() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    for n in 0..SYNC_NOTICE_PATHS + 5 {
        caller.write(&format!("gen/f{n:02}.txt"), "x\n");
    }

    // When
    let sync = merged(worktree.sync_with_caller().await.expect("sync"));

    // Then
    assert_eq!(
        (sync.paths.len(), sync.more_paths, sync.files.created),
        (SYNC_NOTICE_PATHS, 5, (SYNC_NOTICE_PATHS + 5) as u32)
    );
}

#[tokio::test]
async fn uncommitted_conversation_changes_are_committed_as_subagent_work_first() {
    // Given — a background job wrote into the conversation worktree after the last tool call
    let caller = a_caller_worktree().build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    write(worktree.root(), "late.txt", b"late\n");
    caller.write("caller.txt", "theirs\n");

    // When
    worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (
            first_parent_subjects(&worktree),
            read_in(&worktree, "late.txt")
        ),
        (
            vec![
                "Write".to_string(),
                OUTSIDE_A_TOOL_CALL_SUBJECT.to_string(),
                "Merge the caller's changes".to_string()
            ],
            "late\n".to_string()
        )
    );
}

#[tokio::test]
async fn a_sync_leaves_the_callers_index_branch_and_files_untouched() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\n")
        .with_staged_edit("src/lib.rs", "one\nstaged\n")
        .build();
    let worktree = a_conversation_that_committed(&caller, &[("a.txt", "a\n")]).await;
    caller.write("notes.md", "draft\n");
    let before = (
        caller.head(),
        caller.current_branch(),
        caller.index_tree(),
        caller.status(),
    );

    // When
    worktree.sync_with_caller().await.expect("sync");

    // Then
    assert_eq!(
        (
            caller.head(),
            caller.current_branch(),
            caller.index_tree(),
            caller.status()
        ),
        before
    );
}
