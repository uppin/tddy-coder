//! A conversation's own worktree: how it is cut, what one commit per change records, how the work
//! is handed back to the caller, and how it is removed.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md

mod support;

use pretty_assertions::assert_eq;
use support::{
    a_caller_worktree, conversation, file_at, remove, rev_parse, short_head, subjects_after, write,
};
use tddy_subagent_worktree::{
    FileCounts, LineCounts, PullOutcome, WorktreeChange, SUBAGENT_WORKTREES_DIR,
};

#[tokio::test]
async fn looking_up_a_conversation_that_never_wrote_creates_nothing() {
    // Given
    let caller = a_caller_worktree().build();
    let branches_before = caller.branches();

    // When
    let found = caller
        .conversations()
        .existing(&conversation("explore"))
        .await
        .expect("lookup");

    // Then
    assert_eq!(
        (
            found.is_none(),
            caller.exists(SUBAGENT_WORKTREES_DIR),
            caller.branches()
        ),
        (true, false, branches_before)
    );
}

#[tokio::test]
async fn the_worktree_is_cut_from_the_callers_head_on_its_own_branch() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "pub fn a() {}\n")
        .build();

    // When
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");

    // Then
    assert_eq!(
        (
            worktree.root().to_path_buf(),
            worktree.branch().to_string(),
            worktree.base().to_string(),
            std::fs::read_to_string(worktree.root().join("src/lib.rs")).unwrap(),
        ),
        (
            caller.path().join("tmp/subagent-worktrees/explore"),
            "tddy/subagent/sess-1/explore".to_string(),
            caller.head(),
            "pub fn a() {}\n".to_string(),
        )
    );
}

#[tokio::test]
async fn the_callers_uncommitted_and_untracked_changes_become_one_commit_on_the_subagent_branch() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "pub fn a() {}\n")
        .with_committed_file("src/main.rs", "fn main() {}\n")
        .with_staged_edit("src/lib.rs", "pub fn a() { staged }\n")
        .with_unstaged_edit("src/main.rs", "fn main() { unstaged }\n")
        .with_untracked_file("notes.md", "draft\n")
        .build();
    let head = caller.head();

    // When
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");

    // Then
    let base = worktree.base().to_string();
    assert_eq!(
        (
            rev_parse(worktree.root(), &format!("{base}^")),
            file_at(worktree.root(), &base, "src/lib.rs"),
            file_at(worktree.root(), &base, "src/main.rs"),
            file_at(worktree.root(), &base, "notes.md"),
        ),
        (
            head,
            "pub fn a() { staged }\n".to_string(),
            "fn main() { unstaged }\n".to_string(),
            "draft\n".to_string(),
        )
    );
}

#[tokio::test]
async fn the_inherited_commit_leaves_out_ignored_files() {
    // Given
    let caller = a_caller_worktree()
        .with_ignored_file("target/debug.log", "noise\n")
        .with_untracked_file("notes.md", "draft\n")
        .build();

    // When
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");

    // Then
    assert_eq!(worktree.root().join("target/debug.log").exists(), false);
}

#[tokio::test]
async fn cutting_the_worktree_leaves_the_callers_index_branch_and_files_untouched() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "pub fn a() {}\n")
        .with_staged_edit("src/lib.rs", "pub fn a() { staged }\n")
        .with_untracked_file("notes.md", "draft\n")
        .build();
    let before = (
        caller.head(),
        caller.current_branch(),
        caller.index_tree(),
        caller.read("src/lib.rs"),
        caller.read("notes.md"),
    );

    // When
    caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");

    // Then
    assert_eq!(
        (
            caller.head(),
            caller.current_branch(),
            caller.index_tree(),
            caller.read("src/lib.rs"),
            caller.read("notes.md"),
        ),
        before
    );
}

#[tokio::test]
async fn the_conversation_worktree_never_shows_in_the_callers_status() {
    // Given
    let caller = a_caller_worktree().build();
    let status_before = caller.status();

    // When
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "src/new.rs", b"pub fn new() {}\n");

    // Then
    assert_eq!(caller.status(), status_before);
}

#[tokio::test]
async fn a_linked_session_worktree_gets_its_conversation_worktree_in_its_own_tmp() {
    // Given
    let caller = a_caller_worktree().in_a_linked_worktree().build();

    // When
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");

    // Then
    assert_eq!(
        (worktree.root().to_path_buf(), caller.status()),
        (caller.path().join("tmp/subagent-worktrees/explore"), vec![])
    );
}

#[tokio::test]
async fn ensuring_twice_finds_the_same_worktree_and_base() {
    // Given
    let caller = a_caller_worktree()
        .with_untracked_file("notes.md", "draft\n")
        .build();
    let conversations = caller.conversations();
    let first = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("first ensure");
    caller.write("notes.md", "the caller kept typing\n");

    // When
    let second = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("second ensure");

    // Then
    assert_eq!(
        (second.root().to_path_buf(), second.base().to_string()),
        (first.root().to_path_buf(), first.base().to_string())
    );
}

#[tokio::test]
async fn two_sessions_can_hold_conversations_of_the_same_name() {
    // Given
    let caller = a_caller_worktree().in_a_linked_worktree().build();
    let other_session = tddy_subagent_worktree::ConversationWorktrees::new(caller.path(), "sess-2");

    // When
    let mine = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure sess-1");
    let theirs_branch = other_session.branch_of(&conversation("explore"));

    // Then
    assert_eq!(
        (mine.branch().to_string(), theirs_branch),
        (
            "tddy/subagent/sess-1/explore".to_string(),
            "tddy/subagent/sess-2/explore".to_string()
        )
    );
}

#[tokio::test]
async fn a_change_is_committed_with_its_created_updated_and_removed_counts() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\n")
        .with_committed_file("src/old.rs", "a\nb\nc\n")
        .build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "src/lib.rs", b"one\nTWO\nthree\n");
    write(worktree.root(), "src/new.rs", b"x\ny\n");
    remove(worktree.root(), "src/old.rs");

    // When
    let change = worktree.commit_changes("Write").await.expect("commit");

    // Then
    assert_eq!(
        change,
        WorktreeChange {
            commit: Some(short_head(worktree.root())),
            files: FileCounts {
                created: 1,
                updated: 1,
                removed: 1
            },
            lines: LineCounts {
                added: 4,
                removed: 4
            },
        }
    );
}

#[tokio::test]
async fn each_commit_is_named_after_the_tool_that_made_it() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "a.txt", b"a\n");
    worktree
        .commit_changes("Write")
        .await
        .expect("first commit");
    write(worktree.root(), "b.txt", b"b\n");

    // When
    worktree
        .commit_changes("Shell")
        .await
        .expect("second commit");

    // Then
    assert_eq!(
        subjects_after(worktree.root(), worktree.base(), worktree.branch()),
        vec!["Write".to_string(), "Shell".to_string()]
    );
}

#[tokio::test]
async fn a_call_that_changed_nothing_makes_no_commit() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    let tip_before = rev_parse(worktree.root(), "HEAD");

    // When
    let change = worktree.commit_changes("Shell").await.expect("commit");

    // Then
    assert_eq!(
        (change, rev_parse(worktree.root(), "HEAD")),
        (WorktreeChange::default(), tip_before)
    );
}

#[tokio::test]
async fn a_binary_file_counts_as_a_file_and_adds_no_lines() {
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
        &[0x89, b'P', b'N', b'G', 0x00, 0x01, 0x02],
    );

    // When
    let change = worktree.commit_changes("Write").await.expect("commit");

    // Then
    assert_eq!(
        (change.files, change.lines),
        (
            FileCounts {
                created: 1,
                updated: 0,
                removed: 0
            },
            LineCounts::default()
        )
    );
}

#[tokio::test]
async fn the_subagents_commits_are_not_attributed_to_the_developer() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "a.txt", b"a\n");

    // When
    worktree.commit_changes("Write").await.expect("commit");

    // Then
    assert_eq!(
        support::git(worktree.root(), &["log", "-1", "--format=%an <%ae>"]).trim(),
        "tddy-subagent <tddy-subagent@tddy.invalid>"
    );
}

#[tokio::test]
async fn pulling_applies_everything_since_the_base_as_uncommitted_changes() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\n")
        .build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "src/lib.rs", b"one\ntwo\n");
    worktree.commit_changes("StrReplace").await.expect("first");
    write(worktree.root(), "src/new.rs", b"new\n");
    worktree.commit_changes("Write").await.expect("second");

    // When
    let pulled = worktree.pull_into_caller().await.expect("pull");

    // Then
    assert_eq!(
        (pulled, caller.read("src/lib.rs"), caller.read("src/new.rs")),
        (
            PullOutcome {
                files: FileCounts {
                    created: 1,
                    updated: 1,
                    removed: 0
                },
                lines: LineCounts {
                    added: 2,
                    removed: 0
                },
                conflicts: vec![],
            },
            "one\ntwo\n".to_string(),
            "new\n".to_string()
        )
    );
}

#[tokio::test]
async fn pulling_does_not_reapply_the_callers_own_inherited_changes() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\n")
        .with_unstaged_edit("src/lib.rs", "one\ncaller\n")
        .build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "src/new.rs", b"new\n");
    worktree.commit_changes("Write").await.expect("commit");

    // When
    let pulled = worktree.pull_into_caller().await.expect("pull");

    // Then
    assert_eq!(
        (pulled.conflicts, caller.read("src/lib.rs")),
        (Vec::<String>::new(), "one\ncaller\n".to_string())
    );
}

#[tokio::test]
async fn a_pull_over_the_callers_own_edit_leaves_conflict_markers_and_names_the_path() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "one\ntwo\nthree\n")
        .build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "src/lib.rs", b"one\nsubagent\nthree\n");
    worktree.commit_changes("StrReplace").await.expect("commit");
    caller.write("src/lib.rs", "one\ncaller\nthree\n");

    // When
    let pulled = worktree.pull_into_caller().await.expect("pull");

    // Then — the marker labels vary by git version, so the file is checked for the markers and both
    // sides rather than for one exact rendering
    let merged = caller.read("src/lib.rs");
    assert_eq!(
        (
            pulled.conflicts,
            merged.contains("<<<<<<<"),
            merged.contains("caller"),
            merged.contains("subagent")
        ),
        (vec!["src/lib.rs".to_string()], true, true, true)
    );
}

#[tokio::test]
async fn pulling_never_moves_the_callers_head() {
    // Given
    let caller = a_caller_worktree().build();
    let head = caller.head();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "a.txt", b"a\n");
    worktree.commit_changes("Write").await.expect("commit");

    // When
    worktree.pull_into_caller().await.expect("pull");

    // Then
    assert_eq!(
        (caller.head(), caller.current_branch()),
        (head, "master".to_string())
    );
}

#[tokio::test]
async fn removing_deletes_the_worktree_and_its_branch() {
    // Given
    let caller = a_caller_worktree().build();
    let branches_before = caller.branches();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    let root = worktree.root().to_path_buf();

    // When
    worktree.remove().await.expect("remove");

    // Then
    assert_eq!(
        (
            root.exists(),
            caller.branches(),
            caller
                .conversations()
                .existing(&conversation("explore"))
                .await
                .expect("lookup")
                .is_none()
        ),
        (false, branches_before, true)
    );
}

#[tokio::test]
async fn a_deleted_worktree_directory_is_recreated_on_its_surviving_branch() {
    // Given
    let caller = a_caller_worktree().build();
    let conversations = caller.conversations();
    let first = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("first ensure");
    write(
        first.root(),
        "kept.md",
        b"committed before the directory vanished\n",
    );
    first.commit_changes("Write").await.expect("commit");
    caller.delete_directory("tmp/subagent-worktrees/explore");

    // When
    let again = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("ensure after the directory was deleted");

    // Then
    assert_eq!(
        (
            std::fs::read_to_string(again.root().join("kept.md")).unwrap(),
            again.base().to_string()
        ),
        (
            "committed before the directory vanished\n".to_string(),
            first.base().to_string()
        )
    );
}

#[tokio::test]
async fn a_surviving_branch_whose_base_was_lost_starts_the_conversation_afresh() {
    // Given
    let caller = a_caller_worktree().build();
    let conversations = caller.conversations();
    let first = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("first ensure");
    write(
        first.root(),
        "orphaned.md",
        b"no known base to hand back against\n",
    );
    first.commit_changes("Write").await.expect("commit");
    caller.delete_directory("tmp/subagent-worktrees/explore");
    caller.lose_base_ref_of(&conversation("explore"));

    // When
    let again = conversations
        .ensure(&conversation("explore"))
        .await
        .expect("ensure after the base was lost");

    // Then
    assert_eq!(
        (
            again.root().join("orphaned.md").exists(),
            again.base().to_string()
        ),
        (false, caller.head())
    );
}

#[tokio::test]
async fn two_concurrent_ensures_of_one_conversation_create_it_once() {
    // Given
    let caller = a_caller_worktree().build();
    let conversations = caller.conversations();

    let explore = conversation("explore");

    // When
    let (one, other) = tokio::join!(
        conversations.ensure(&explore),
        conversations.ensure(&explore)
    );

    // Then
    assert_eq!(
        (
            one.expect("first ensure").root().to_path_buf(),
            other.expect("second ensure").root().to_path_buf()
        ),
        (
            caller.path().join("tmp/subagent-worktrees/explore"),
            caller.path().join("tmp/subagent-worktrees/explore")
        )
    );
}

#[tokio::test]
async fn two_concurrent_commits_in_one_conversation_both_succeed_and_keep_every_file() {
    // Given
    let caller = a_caller_worktree().build();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "a.md", b"a\n");
    write(worktree.root(), "b.md", b"b\n");

    // When
    let (one, other) = tokio::join!(
        worktree.commit_changes("Write a"),
        worktree.commit_changes("Write b"),
    );

    // Then
    let files_changed =
        one.expect("first commit").files.created + other.expect("second commit").files.created;
    assert_eq!(
        (
            files_changed,
            file_at(worktree.root(), "HEAD", "a.md"),
            file_at(worktree.root(), "HEAD", "b.md")
        ),
        (2, "a\n".to_string(), "b\n".to_string())
    );
}

#[tokio::test]
async fn a_developers_post_commit_hook_does_not_run_for_a_subagents_commit() {
    // Given
    let caller = a_caller_worktree().build();
    caller.install_post_commit_hook_leaving_hook_ran();
    let worktree = caller
        .conversations()
        .ensure(&conversation("explore"))
        .await
        .expect("ensure");
    write(worktree.root(), "new.md", b"new\n");

    // When
    worktree.commit_changes("Write").await.expect("commit");

    // Then
    assert_eq!(caller.exists("hook-ran"), false);
}
