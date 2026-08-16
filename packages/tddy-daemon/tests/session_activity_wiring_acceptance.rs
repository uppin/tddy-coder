//! Reading HEAD without a subprocess, and what one poll tick produces — AC1 and AC13 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Real git repositories in temp directories. `read_head_commit` is checked against what git itself
//! reports for the same checkout, so the test cannot agree with a wrong implementation by sharing
//! its mistake.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_daemon::session_room::{
    read_head_commit, snapshot_worktree, tick_delta, write_wip_tree_within, WorktreeSnapshot,
};

const A_GENEROUS_BUDGET: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A checkout with one commit on `main`.
fn a_session_worktree(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// A repository with no commit at all — the state between `git init` and the first commit.
fn an_unborn_repository(root: &Path) {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// What git itself says HEAD is, which is the only thing worth comparing against.
fn head_according_to_git(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn a_snapshot_with_tree(head_commit: &str, wip_tree: &str) -> WorktreeSnapshot {
    WorktreeSnapshot {
        head_commit: head_commit.to_string(),
        wip_tree: wip_tree.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Reading HEAD
// ---------------------------------------------------------------------------

#[test]
fn reads_the_commit_a_checkout_is_on() {
    // Given an ordinary checkout on a branch
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());

    // When
    let head = read_head_commit(repo.path());

    // Then it agrees with git, which is the whole contract — this exists only to answer the same
    // question `rev-parse HEAD` answers, without paying for a process on every tool call.
    assert_eq!(head, head_according_to_git(repo.path()));
}

#[test]
fn reads_a_detached_head() {
    // Given a checkout detached onto a commit
    let repo = tempfile::tempdir().expect("tempdir");
    let first = a_session_worktree(repo.path());
    std::fs::write(repo.path().join("second.txt"), "two\n").expect("write");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "second"]);
    git(repo.path(), &["checkout", "--detach", &first]);

    // When
    let head = read_head_commit(repo.path());

    // Then the sha is read straight out of HEAD rather than followed as a ref.
    assert_eq!(head, first);
}

#[test]
fn reads_a_head_only_packed_refs_still_knows() {
    // Given a checkout whose branch ref has been packed away, which `git gc` does routinely
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());
    git(repo.path(), &["pack-refs", "--all"]);

    // When
    let head = read_head_commit(repo.path());

    // Then it resolves anyway. A loose-ref-only reader would answer "" on any repository that has
    // been garbage-collected, which is every long-lived one.
    assert_eq!(head, head_according_to_git(repo.path()));
}

#[test]
fn reads_the_head_of_a_linked_git_worktree() {
    // Given a session worktree, which is a `git worktree` of the project's main checkout — so its
    // `.git` is a FILE naming the real gitdir, not a directory
    let project = tempfile::tempdir().expect("tempdir");
    a_session_worktree(project.path());
    let worktrees = tempfile::tempdir().expect("tempdir");
    let linked = worktrees.path().join("session-abc");
    git(
        project.path(),
        &[
            "worktree",
            "add",
            "-b",
            "feat/session",
            &linked.to_string_lossy(),
        ],
    );
    assert!(
        linked.join(".git").is_file(),
        "fixture must produce a linked worktree, whose .git is a file"
    );

    // When
    let head = read_head_commit(&linked);

    // Then the gitdir indirection is followed. This is the shape every session worktree has, so a
    // reader that only understood a `.git` directory would answer "" for all of them.
    assert_eq!(head, head_according_to_git(&linked));
}

#[test]
fn reads_the_head_of_a_linked_worktree_whose_ref_is_packed_in_the_common_dir() {
    // Given a linked worktree whose refs live packed in the MAIN repository's packed-refs
    let project = tempfile::tempdir().expect("tempdir");
    a_session_worktree(project.path());
    let worktrees = tempfile::tempdir().expect("tempdir");
    let linked = worktrees.path().join("session-abc");
    git(
        project.path(),
        &[
            "worktree",
            "add",
            "-b",
            "feat/session",
            &linked.to_string_lossy(),
        ],
    );
    git(project.path(), &["pack-refs", "--all"]);

    // When
    let head = read_head_commit(&linked);

    // Then it is found in the COMMON dir's packed-refs, not beside the worktree's own HEAD —
    // a linked worktree has its own HEAD but shares the repository's refs.
    assert_eq!(head, head_according_to_git(&linked));
}

#[test]
fn reports_an_unborn_branch_as_no_commit_rather_than_inventing_one() {
    // Given a repository with no commit yet
    let repo = tempfile::tempdir().expect("tempdir");
    an_unborn_repository(repo.path());

    // When
    let head = read_head_commit(repo.path());

    // Then it is empty. A fabricated sha would make every record claim a base commit that does not
    // exist, and a mirror trusting it would be confidently wrong rather than merely uninformed.
    assert_eq!(head, "");
}

#[test]
fn reports_a_directory_that_is_not_a_repository_as_no_commit() {
    // Given a plain directory
    let not_a_repo = tempfile::tempdir().expect("tempdir");

    // When
    let head = read_head_commit(not_a_repo.path());

    // Then
    assert_eq!(head, "");
}

#[test]
fn agrees_with_the_snapshot_the_poll_loop_takes_of_the_same_checkout() {
    // Given a checkout the poll loop has measured
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let snapshot = snapshot_worktree(repo.path());

    // When the cheap reader answers the same question
    let read = read_head_commit(repo.path());

    // Then the two agree, and both agree with git. Comparing them to each other alone would pass
    // on two empty strings, which is what a wholly broken reader returns.
    assert_eq!(read, head);
    assert_eq!(snapshot.head_commit, head);
}

// ---------------------------------------------------------------------------
// What one tick produces
// ---------------------------------------------------------------------------

#[test]
fn produces_a_delta_when_the_working_tree_moved() {
    // Given two ticks with a write between them
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("new.txt"), "hello\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&head, &before),
        &a_snapshot_with_tree(&head, &after),
        7,
    )
    .expect("a moved tree must produce a delta");

    // Then it is numbered and based where the tick found the checkout.
    assert_eq!(delta.seq, 7);
    assert_eq!(delta.prev_seq, 6);
    assert_eq!(delta.base_commit, head);
    assert!(
        !delta.patch.is_empty(),
        "a tree that moved must produce a non-empty patch"
    );
}

#[test]
fn produces_no_delta_when_the_working_tree_did_not_move() {
    // Given two ticks that measured the same tree
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let tree = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&head, &tree),
        &a_snapshot_with_tree(&head, &tree),
        7,
    );

    // Then there is no tick to hand out. An idle checkout polled every two seconds would otherwise
    // fill the store with empty deltas and evict the ones a client still needs.
    assert_eq!(delta, None);
}

#[test]
fn produces_no_delta_on_the_first_tick_because_there_is_nothing_to_diff_from() {
    // Given a first tick, whose predecessor measured no tree at all
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&head, ""),
        &a_snapshot_with_tree(&head, &after),
        1,
    );

    // Then nothing. A client attaching mid-session catches up by fetching the WIP ref, not by
    // being handed a delta against a tree that was never measured.
    assert_eq!(delta, None);
}

#[test]
fn produces_no_delta_when_the_measurement_failed() {
    // Given a tick whose tree could not be written
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&head, &before),
        &a_snapshot_with_tree(&head, ""),
        2,
    );

    // Then no delta rather than a wrong one — a failed measurement is a tick of lost freshness,
    // never a diff against a tree the checkout does not have.
    assert_eq!(delta, None);
}

#[test]
fn bases_a_delta_on_the_commit_the_tick_ended_at_when_the_agent_committed() {
    // Given a tick across a commit, so HEAD moved between the two measurements
    let repo = tempfile::tempdir().expect("tempdir");
    let first = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("new.txt"), "hello\n").expect("write");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "the agent committed"]);
    let second = head_according_to_git(repo.path());
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&first, &before),
        &a_snapshot_with_tree(&second, &after),
        3,
    )
    .expect("a commit moves the tree and must produce a delta");

    // Then it names where the checkout ended up, not where it started. A client applies a delta
    // after following HEAD, so basing it on the old commit would reject on every commit.
    assert_eq!(delta.base_commit, second);
}

#[test]
fn produces_an_unscoped_delta_because_scoping_happens_at_lookup() {
    // Given a tick that moved
    let repo = tempfile::tempdir().expect("tempdir");
    let head = a_session_worktree(repo.path());
    let before = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);
    std::fs::write(repo.path().join("new.txt"), "hello\n").expect("write");
    let after = write_wip_tree_within(repo.path(), A_GENEROUS_BUDGET);

    // When
    let delta = tick_delta(
        repo.path(),
        &a_snapshot_with_tree(&head, &before),
        &a_snapshot_with_tree(&head, &after),
        1,
    )
    .expect("a moved tree must produce a delta");

    // Then it carries the whole window. The store slices it per call on the way out, and a delta
    // recorded already narrowed could never be sliced into the calls that share its tick.
    assert_eq!(delta.scoped_paths, Vec::<String>::new());
}
