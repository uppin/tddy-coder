//! What a session room says, and when: room naming, git snapshotting, and the rules that turn two
//! consecutive snapshots into broadcast events.
//!
//! Product contract: `docs/ft/daemon/session-room.md`
//!
//! No LiveKit here. The event rules are pure over a pair of snapshots, and snapshotting is `git`
//! against a real checkout in a tempdir — shelling out to git *is* the behaviour under test, so a fake
//! would only test the fake.

use std::path::{Path, PathBuf};

use tddy_daemon::session_room::{
    activity_between, room_metadata_json, session_room_name, snapshot_worktree, WorktreeSnapshot,
};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};
use tddy_service::worktree_activity::format_worktree_activity_for_log;

/// A fixed instant, so a test never asserts against the wall clock.
const AT: u64 = 1_760_000_000_000;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A clean checkout sitting on `main` at one commit: the state a freshly created worktree is in.
fn a_snapshot() -> SnapshotBuilder {
    SnapshotBuilder {
        head_commit: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".to_string(),
        branch: "main".to_string(),
        changed_paths: Vec::new(),
        changed_files: 0,
        lines_added: 0,
        lines_removed: 0,
        untracked_files: 0,
    }
}

struct SnapshotBuilder {
    head_commit: String,
    branch: String,
    changed_paths: Vec<String>,
    changed_files: u32,
    lines_added: i64,
    lines_removed: i64,
    untracked_files: u32,
}

impl SnapshotBuilder {
    fn at_commit(mut self, sha: &str) -> Self {
        self.head_commit = sha.to_string();
        self
    }

    fn on_branch(mut self, branch: &str) -> Self {
        self.branch = branch.to_string();
        self
    }

    /// The working-tree diff as `git diff --numstat HEAD` reports it, paths included.
    fn with_diff(mut self, paths: &[&str], added: i64, removed: i64) -> Self {
        self.changed_paths = paths.iter().map(|p| p.to_string()).collect();
        self.changed_files = paths.len() as u32;
        self.lines_added = added;
        self.lines_removed = removed;
        self
    }

    fn with_untracked(mut self, count: u32) -> Self {
        self.untracked_files = count;
        self
    }

    fn build(self) -> WorktreeSnapshot {
        WorktreeSnapshot {
            head_commit: self.head_commit,
            branch: self.branch,
            changed_paths: self.changed_paths,
            changed_files: self.changed_files,
            lines_added: self.lines_added,
            lines_removed: self.lines_removed,
            untracked_files: self.untracked_files,
            // This suite is about which events a pair of snapshots produces, which the WIP tree
            // takes no part in — it is what a delta is cut from, not what an event is derived from.
            wip_tree: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Room naming
// ---------------------------------------------------------------------------

#[test]
fn a_room_is_named_after_the_session_it_serves() {
    // Given the id of a session
    let session_id = "0198f3c1-2a4b-7c8d-9e0f-112233445566";

    // When its room is named
    let room = session_room_name(session_id);

    // Then the name is derived from the id alone, so the facilitating daemon never has to be told
    // it — the room belongs to the session, not to the checkout the session happens to work in
    assert_eq!(room, "session-0198f3c1-2a4b-7c8d-9e0f-112233445566");
}

// ---------------------------------------------------------------------------
// Snapshot → events
// ---------------------------------------------------------------------------

#[test]
fn a_new_head_commit_is_reported_as_a_commit_event() {
    // Given a checkout whose HEAD has moved
    let before = a_snapshot()
        .at_commit("1111111111111111111111111111111111111111")
        .build();
    let after = a_snapshot()
        .at_commit("2222222222222222222222222222222222222222")
        .build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 7, AT);

    // Then one commit event carries the new sha
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind(), WorktreeActivityKind::Commit);
    assert_eq!(
        events[0].head_commit,
        "2222222222222222222222222222222222222222"
    );
    assert_eq!(events[0].seq, 7);
    assert_eq!(events[0].at_unix_ms, AT);
}

#[test]
fn a_changed_working_tree_summary_is_reported_as_a_files_changed_event() {
    // Given a checkout where two lines were written into one tracked file
    let before = a_snapshot().build();
    let after = a_snapshot().with_diff(&["src/lib.rs"], 2, 0).build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 1, AT);

    // Then one files-changed event carries the counts, and no path
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind(), WorktreeActivityKind::FilesChanged);
    assert_eq!(events[0].changed_files, 1);
    assert_eq!(events[0].lines_added, 2);
    assert_eq!(events[0].lines_removed, 0);
}

#[test]
fn an_unchanged_snapshot_produces_no_activity() {
    // Given two identical snapshots — what every poll over an idle worktree sees
    let snapshot = a_snapshot().with_diff(&["src/lib.rs"], 2, 0).build();

    // When they are compared
    let events = activity_between(&snapshot, &snapshot, 1, AT);

    // Then nothing is published. Polling is how change is detected, so an idle poll that announced
    // anything would flood the room at the poll rate.
    assert_eq!(events, Vec::new());
}

#[test]
fn a_commit_that_also_clears_the_working_tree_reports_the_commit_then_the_working_tree() {
    // Given a checkout where staged work was committed, leaving the working tree clean
    let before = a_snapshot()
        .at_commit("1111111111111111111111111111111111111111")
        .with_diff(&["src/lib.rs"], 4, 1)
        .build();
    let after = a_snapshot()
        .at_commit("2222222222222222222222222222222222222222")
        .build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 10, AT);

    // Then both facts are announced, each with its own sequence number — a receiver that only
    // understood one kind would otherwise silently miss the other
    assert_eq!(
        events.iter().map(|e| e.kind()).collect::<Vec<_>>(),
        vec![
            WorktreeActivityKind::Commit,
            WorktreeActivityKind::FilesChanged
        ]
    );
    assert_eq!(
        events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![10, 11]
    );
    assert_eq!(events[1].changed_files, 0);
    assert_eq!(events[1].lines_added, 0);
    assert_eq!(events[1].lines_removed, 0);
}

#[test]
fn switching_to_a_branch_at_another_commit_is_announced_as_that_commit_alone() {
    // Given a checkout moved onto another branch that happens to point at a different commit
    let before = a_snapshot()
        .on_branch("main")
        .at_commit("1111111111111111111111111111111111111111")
        .build();
    let after = a_snapshot()
        .on_branch("feat-x")
        .at_commit("3333333333333333333333333333333333333333")
        .build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 1, AT);

    // Then only the moved HEAD is announced: the branch name is state, carried in room metadata,
    // not an event of its own
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind(), WorktreeActivityKind::Commit);
}

#[test]
fn a_branch_cut_at_the_commit_already_checked_out_produces_no_activity() {
    // Given a checkout moved onto a branch pointing at the commit it was already on — what
    // `git switch -c` leaves behind
    let before = a_snapshot()
        .on_branch("main")
        .at_commit("5555555555555555555555555555555555555555")
        .build();
    let after = a_snapshot()
        .on_branch("feat-x")
        .at_commit("5555555555555555555555555555555555555555")
        .build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 1, AT);

    // Then nothing is announced. No commit landed and no file moved, so the only thing that changed
    // is the name the checkout answers to — state a joiner reads from room metadata, and nothing a
    // receiver could act on.
    assert_eq!(events, Vec::new());
}

#[test]
fn a_new_untracked_file_alone_produces_no_activity() {
    // Given two polls that differ only in a file git has never been told about
    let before = a_snapshot().with_untracked(0).build();
    let after = a_snapshot().with_untracked(1).build();

    // When the two snapshots are compared
    let events = activity_between(&before, &after, 4, AT);

    // Then nothing is published. An event carries counts only and `git diff --numstat HEAD` has none
    // for a path it does not track, so the sole event possible would be `files=0 +0 -0` — and since
    // every write is untracked for the instant before `git add`, that empty event would arrive right
    // in front of the commit that actually describes the work. The fact is not lost: the untracked
    // count lives in the room's metadata.
    assert_eq!(events, Vec::new());
}

// ---------------------------------------------------------------------------
// Room metadata
// ---------------------------------------------------------------------------

#[test]
fn room_metadata_carries_the_changed_file_list_the_events_leave_out() {
    // Given a snapshot with uncommitted work
    let snapshot = a_snapshot()
        .on_branch("feat-worktree-room")
        .at_commit("4444444444444444444444444444444444444444")
        .with_diff(&["src/lib.rs", "README.md"], 12, 3)
        .with_untracked(2)
        .build();

    // When the room's metadata is rendered
    let metadata: serde_json::Value = serde_json::from_str(&room_metadata_json(
        &snapshot,
        &["brief.md".to_string()],
        AT,
    ))
    .expect("room metadata must be JSON");

    // Then it is the full picture a joining agent needs without a single round trip — including the
    // paths, which is exactly what the broadcast omits
    assert_eq!(
        metadata,
        serde_json::json!({
            "head_commit": "4444444444444444444444444444444444444444",
            "branch": "feat-worktree-room",
            "changed_paths": ["src/lib.rs", "README.md"],
            "changed_files": 2,
            "lines_added": 12,
            "lines_removed": 3,
            "untracked_files": 2,
            "attachments": ["brief.md"],
            "updated_at_unix_ms": AT,
        })
    );
}

// ---------------------------------------------------------------------------
// The DEBUG line every receiver emits
// ---------------------------------------------------------------------------

#[test]
fn a_received_event_renders_as_one_debug_line_naming_its_kind_sequence_and_counts() {
    // Given a files-changed event as it arrives off the wire
    let before = a_snapshot().build();
    let after = a_snapshot().with_diff(&["src/lib.rs"], 9, 4).build();
    let event = activity_between(&before, &after, 3, AT)
        .into_iter()
        .next()
        .expect("a changed working tree must produce an event");

    // When it is rendered for the log
    let line = format_worktree_activity_for_log(&event);

    // Then one line says what happened. This is the whole of what a receiver does with an event
    // today, so it is the whole of what there is to pin.
    assert_eq!(line, "worktree activity: files_changed seq=3 files=1 +9 -4");
}

#[test]
fn a_commit_event_renders_with_its_short_sha() {
    // Given a commit event
    let before = a_snapshot()
        .at_commit("1111111111111111111111111111111111111111")
        .build();
    let after = a_snapshot()
        .at_commit("abcdef1234567890abcdef1234567890abcdef12")
        .build();
    let event = activity_between(&before, &after, 12, AT)
        .into_iter()
        .next()
        .expect("a moved HEAD must produce an event");

    // When it is rendered for the log
    let line = format_worktree_activity_for_log(&event);

    // Then the sha is abbreviated — a full 40 characters in every line of a busy log buys nothing a
    // reader can use
    assert_eq!(line, "worktree activity: commit seq=12 head=abcdef1");
}

#[test]
fn an_event_of_a_kind_this_build_does_not_know_renders_with_its_raw_wire_value() {
    // Given an event a newer daemon published under a kind this build has no name for
    let event = WorktreeActivityEvent {
        kind: 99,
        seq: 41,
        at_unix_ms: AT,
        head_commit: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        changed_files: 3,
        lines_added: 8,
        lines_removed: 2,
    };

    // When it is rendered for the log
    let line = format_worktree_activity_for_log(&event);

    // Then the line still names the kind, as the number that crossed the wire. An unreadable event
    // is worth knowing about: silence here would make a newer publisher indistinguishable from a
    // dead one, which is the whole diagnosis this log exists to support.
    assert_eq!(line, "worktree activity: unrecognized kind=99 seq=41");
}

// ---------------------------------------------------------------------------
// Snapshotting a real checkout
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A checkout on `main` with one committed file, `tracked.txt`, holding two lines.
fn a_checkout() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "t@t.com"]);
    git(&path, &["config", "user.name", "Test"]);
    std::fs::write(path.join("tracked.txt"), "one\ntwo\n").expect("seed the checkout");
    git(&path, &["add", "tracked.txt"]);
    git(&path, &["commit", "-m", "seed"]);
    (dir, path)
}

/// The sha `HEAD` resolves to in `dir`.
///
/// Checked rather than trusted, because the subject shares this helper's failure mode: a
/// `snapshot_worktree` whose git could not run reports `head_commit: ""`, so an unchecked helper
/// would hand back `""` too and every assertion below would compare one failure against another.
fn head_commit_of(dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git rev-parse HEAD failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // The sha is whatever git minted at commit time — only its shape can be pinned here.
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "git rev-parse HEAD in {dir:?} answered {sha:?}, which is not a commit sha"
    );
    sha
}

#[test]
fn a_clean_checkout_snapshots_as_its_branch_and_head_with_no_changes() {
    // Given a checkout with nothing uncommitted
    let (_dir, checkout) = a_checkout();

    // When it is snapshotted
    let snapshot = snapshot_worktree(&checkout);

    // Then it reports where it is and that nothing has moved
    assert_eq!(snapshot.head_commit, head_commit_of(&checkout));
    assert_eq!(snapshot.branch, "main");
    assert_eq!(snapshot.changed_files, 0);
    assert_eq!(snapshot.changed_paths, Vec::<String>::new());
    assert_eq!(snapshot.lines_added, 0);
    assert_eq!(snapshot.lines_removed, 0);
}

#[test]
fn writing_into_a_tracked_file_snapshots_the_path_and_the_line_counts() {
    // Given a checkout where a tracked file gained two lines and lost one
    let (_dir, checkout) = a_checkout();
    std::fs::write(checkout.join("tracked.txt"), "one\nthree\nfour\n").expect("edit the checkout");

    // When it is snapshotted
    let snapshot = snapshot_worktree(&checkout);

    // Then the counts are `git diff --numstat HEAD`'s, and the path is recorded for room metadata
    assert_eq!(snapshot.changed_paths, vec!["tracked.txt".to_string()]);
    assert_eq!(snapshot.changed_files, 1);
    assert_eq!(snapshot.lines_added, 2);
    assert_eq!(snapshot.lines_removed, 1);
}

#[test]
fn an_untracked_file_snapshots_as_a_count_and_not_as_a_diff() {
    // Given a checkout with a brand new file git has never seen
    let (_dir, checkout) = a_checkout();
    std::fs::write(checkout.join("scratch.txt"), "notes\n").expect("add an untracked file");

    // When it is snapshotted
    let snapshot = snapshot_worktree(&checkout);

    // Then it is counted but contributes no lines. `git diff --numstat HEAD` cannot report line
    // counts for a path it does not track, and inventing them would make the room's totals disagree
    // with what the Worktrees screen shows for the same checkout.
    assert_eq!(snapshot.untracked_files, 1);
    assert_eq!(snapshot.changed_files, 0);
    assert_eq!(snapshot.lines_added, 0);
}

#[test]
fn committing_moves_the_snapshots_head_and_clears_its_working_tree() {
    // Given a checkout with an edit that then gets committed
    let (_dir, checkout) = a_checkout();
    let before_committing = head_commit_of(&checkout);
    std::fs::write(checkout.join("tracked.txt"), "one\ntwo\nthree\n").expect("edit the checkout");
    git(&checkout, &["commit", "-am", "third line"]);

    // When it is snapshotted
    let snapshot = snapshot_worktree(&checkout);

    // Then HEAD is the new commit and the working tree is clean — which is why the room's metadata
    // carries the sha: a committed change is invisible in a HEAD-relative diff, so a snapshot that
    // reported the old HEAD would describe the edit as having never happened at all
    assert_eq!(snapshot.head_commit, head_commit_of(&checkout));
    assert_ne!(
        snapshot.head_commit, before_committing,
        "committing must move the snapshot's HEAD off the commit the edit was made against"
    );
    assert_eq!(snapshot.changed_files, 0);
}
