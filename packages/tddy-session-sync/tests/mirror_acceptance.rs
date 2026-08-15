//! The managed destination — AC25-AC26 and AC29-AC32 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Real git repositories in temp directories, real patches produced by real `git diff --binary`.
//! No LiveKit and no daemon: what is under test is what the mirror does with a delta, which is
//! decided entirely by the delta's own fields and the mirror's state.

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_session_sync::{ApplyOutcome, Delta, Mirror, MirrorError, MirrorMarker, ReconcileReason};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_marker() -> MirrorMarker {
    MirrorMarker {
        session_id: "1780828020298-abc".to_string(),
        daemon_instance_id: "udoo-1780828020298".to_string(),
        project: "my-app".to_string(),
        last_seq: 0,
        last_head_commit: String::new(),
    }
}

/// A git repository with one commit, which is the state a mirror is in right after its clone.
fn a_cloned_mirror_repo(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "sync@example.com"]);
    git(root, &["config", "user.name", "Session Sync"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// A mirror this syncer already owns: the clone plus the `.tddy-session-sync.json` that says so.
/// Returns the commit it is on, which the marker also records — a marker naming a commit the
/// repository does not have would be a mirror that has diverged before the test even starts.
fn a_mirror_this_syncer_owns(root: &Path, marker: MirrorMarker) -> String {
    let head = a_cloned_mirror_repo(root);
    let owned = MirrorMarker {
        last_head_commit: head.clone(),
        ..marker
    };
    std::fs::write(
        root.join(tddy_session_sync::MARKER_FILENAME),
        serde_json::to_vec_pretty(&owned).expect("serialize the marker"),
    )
    .expect("write the marker");
    head
}

/// Run git in `cwd`, returning stdout. A failure panics with git's own stderr — a test that hid it
/// would report "the mirror is wrong" when the truth is "the fixture never built".
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

/// A patch that adds `path` with `contents`, produced by git itself rather than hand-written, so
/// what the mirror applies is exactly the shape the daemon will send.
fn a_patch_adding(path: &str, contents: &str) -> Vec<u8> {
    let scratch = tempfile::tempdir().expect("tempdir");
    let root = scratch.path();
    a_cloned_mirror_repo(root);
    std::fs::write(root.join(path), contents).expect("write the new file");
    git(root, &["add", path]);
    git(root, &["diff", "--binary", "--cached"]).into_bytes()
}

fn a_delta_at(seq: u64, base_commit: &str, patch: Vec<u8>) -> Delta {
    Delta {
        seq,
        prev_seq: seq.saturating_sub(1),
        base_commit: base_commit.to_string(),
        patch,
        scoped_paths: Vec::new(),
    }
}

/// The error an operation produced, for a test that is about the refusal rather than the result.
fn refused(dest: &Path, marker: MirrorMarker) -> MirrorError {
    match Mirror::open_or_create(dest, marker) {
        Err(e) => e,
        Ok(_) => panic!("expected {} to be refused", dest.display()),
    }
}

// ---------------------------------------------------------------------------
// Ownership
// ---------------------------------------------------------------------------

#[test]
fn adopts_an_empty_destination() {
    // Given an empty directory
    let dest = tempfile::tempdir().expect("tempdir");

    // When
    let mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must adopt");

    // Then
    assert_eq!(mirror.marker().session_id, "1780828020298-abc");
}

#[test]
fn refuses_a_destination_it_does_not_own() {
    // Given a non-empty directory with no marker — someone's working directory
    let dest = tempfile::tempdir().expect("tempdir");
    std::fs::write(dest.path().join("my-thesis.txt"), "years of work\n").expect("write");

    // When
    let error = refused(dest.path(), a_marker());

    // Then it is refused rather than adopted: everything under a mirror is discarded on the
    // next sync, so adopting a directory silently would destroy it silently.
    assert_eq!(
        error,
        MirrorError::NotOwned {
            path: dest.path().to_path_buf(),
        }
    );
}

#[test]
fn refuses_a_destination_marked_for_another_session() {
    // Given a directory already mirroring a different session
    let dest = tempfile::tempdir().expect("tempdir");
    let other = MirrorMarker {
        session_id: "1780828020298-zzz".to_string(),
        ..a_marker()
    };
    Mirror::open_or_create(dest.path(), other).expect("must adopt for the other session");

    // When it is opened for ours
    let error = refused(dest.path(), a_marker());

    // Then
    assert_eq!(
        error,
        MirrorError::ForeignSession {
            path: dest.path().to_path_buf(),
            expected_session_id: "1780828020298-abc".to_string(),
            found_session_id: "1780828020298-zzz".to_string(),
        }
    );
}

#[test]
fn reopens_a_destination_it_already_owns() {
    // Given a directory this syncer already mirrors, one delta in
    let dest = tempfile::tempdir().expect("tempdir");
    let marker = MirrorMarker {
        last_seq: 7,
        last_head_commit: "c0ffee".to_string(),
        ..a_marker()
    };
    Mirror::open_or_create(dest.path(), marker).expect("must adopt");

    // When it is opened again
    let mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must reopen");

    // Then the persisted progress is what it resumes from, not the freshly built marker —
    // re-deriving it from the room would re-apply every delta since the session began.
    assert_eq!(mirror.marker().last_seq, 7);
    assert_eq!(mirror.marker().last_head_commit, "c0ffee");
}

// ---------------------------------------------------------------------------
// Applying deltas
// ---------------------------------------------------------------------------

#[test]
fn applies_a_delta_that_follows_the_last_one() {
    // Given a mirror at seq 0
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");

    // When
    let outcome = mirror
        .apply(&a_delta_at(1, &head, a_patch_adding("new.txt", "hello\n")))
        .expect("must apply");

    // Then
    assert_eq!(outcome, ApplyOutcome::Applied);
    assert_eq!(mirror.read("new.txt").expect("must read"), b"hello\n");
    assert_eq!(mirror.marker().last_seq, 1);
}

#[test]
fn applies_one_delta_once_when_several_calls_share_a_tick() {
    // Given a mirror that has applied tick 1
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");
    let delta = a_delta_at(1, &head, a_patch_adding("new.txt", "hello\n"));
    mirror.apply(&delta).expect("must apply the first time");

    // When the same tick arrives again — which it does whenever two tool calls landed in one
    // poll window, because each names the same delta
    let outcome = mirror.apply(&delta).expect("must not fail");

    // Then it is recognised as already applied rather than applied twice.
    assert_eq!(outcome, ApplyOutcome::AlreadyApplied);
    assert_eq!(mirror.marker().last_seq, 1);
}

#[test]
fn reconciles_when_a_sequence_number_is_missing() {
    // Given a mirror at tick 1
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(
        dest.path(),
        MirrorMarker {
            last_seq: 1,
            ..a_marker()
        },
    );
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");

    // When tick 3 arrives, so tick 2 was lost in transit
    let outcome = mirror
        .apply(&a_delta_at(3, &head, a_patch_adding("new.txt", "hello\n")))
        .expect("must not fail");

    // Then it is not applied out of order and not skipped over — the intervening tick's changes
    // would be missing from the mirror forever.
    assert_eq!(
        outcome,
        ApplyOutcome::NeedsReconcile(ReconcileReason::SequenceGap {
            expected: 2,
            found: 3,
        })
    );
}

#[test]
fn reconciles_when_a_delta_was_cut_from_another_commit() {
    // Given a mirror on one commit
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");

    // When a delta cut from a different commit arrives
    let outcome = mirror
        .apply(&a_delta_at(
            1,
            "1111111111111111111111111111111111111111",
            a_patch_adding("new.txt", "hello\n"),
        ))
        .expect("must not fail");

    // Then
    assert_eq!(
        outcome,
        ApplyOutcome::NeedsReconcile(ReconcileReason::BaseCommitMismatch {
            expected: head,
            found: "1111111111111111111111111111111111111111".to_string(),
        })
    );
}

#[test]
fn reconciles_when_a_patch_does_not_apply() {
    // Given a mirror whose file has drifted from what the patch expects
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    std::fs::write(dest.path().join("new.txt"), "something else entirely\n").expect("drift");
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");

    // When a patch that creates that same file arrives
    let outcome = mirror
        .apply(&a_delta_at(1, &head, a_patch_adding("new.txt", "hello\n")))
        .expect("must not fail");

    // Then a reconcile is asked for rather than a partial apply.
    assert!(
        matches!(
            outcome,
            ApplyOutcome::NeedsReconcile(ReconcileReason::PatchRejected { .. })
        ),
        "expected a rejected patch, got {outcome:?}"
    );
}

#[test]
fn leaves_the_mirror_untouched_when_a_patch_is_rejected() {
    // Given a mirror with a file the incoming patch will collide with
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    std::fs::write(dest.path().join("new.txt"), "something else entirely\n").expect("drift");
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");

    // When the patch is rejected
    mirror
        .apply(&a_delta_at(1, &head, a_patch_adding("new.txt", "hello\n")))
        .expect("must not fail");

    // Then nothing was written and the sequence did not advance, so the reconcile that follows
    // starts from a state the syncer can describe rather than a half-applied one.
    assert_eq!(
        mirror.read("new.txt").expect("must read"),
        b"something else entirely\n"
    );
    assert_eq!(mirror.marker().last_seq, 0);
}

#[test]
fn names_the_expected_and_actual_values_of_every_divergence() {
    // Given the three reasons a mirror can diverge
    let reasons = [
        ReconcileReason::SequenceGap {
            expected: 2,
            found: 3,
        },
        ReconcileReason::BaseCommitMismatch {
            expected: "aaaa".to_string(),
            found: "bbbb".to_string(),
        },
        ReconcileReason::PatchRejected {
            detail: "error: patch failed: new.txt:1".to_string(),
        },
    ];

    // When each is rendered for the log
    let rendered: Vec<String> = reasons.iter().map(|r| r.to_string()).collect();

    // Then each names what it saw — a reconcile logged as "diverged" with no values is a
    // reconcile nobody can debug.
    assert_eq!(
        rendered,
        vec![
            "activity sequence gap: expected seq 2, received 3".to_string(),
            "delta was cut from bbbb but the mirror is on aaaa".to_string(),
            "git apply refused the patch: error: patch failed: new.txt:1".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// Marker persistence
// ---------------------------------------------------------------------------

#[test]
fn records_progress_in_the_marker_so_a_restart_resumes_rather_than_replays() {
    // Given a mirror that has applied a delta
    let dest = tempfile::tempdir().expect("tempdir");
    let head = a_mirror_this_syncer_owns(dest.path(), a_marker());
    let mut mirror = Mirror::open_or_create(dest.path(), a_marker()).expect("must open");
    mirror
        .apply(&a_delta_at(1, &head, a_patch_adding("new.txt", "hello\n")))
        .expect("must apply");

    // When the marker is read back off disk
    let path: PathBuf = dest.path().join(tddy_session_sync::MARKER_FILENAME);
    let written: MirrorMarker =
        serde_json::from_slice(&std::fs::read(&path).expect("marker must exist"))
            .expect("marker must parse");

    // Then
    assert_eq!(written.last_seq, 1);
    assert_eq!(written.session_id, "1780828020298-abc");
}
