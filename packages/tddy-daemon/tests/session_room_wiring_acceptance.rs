//! What a hosted session room hangs its uncommitted-state surface off — AC4, AC9 and AC13 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Opening a room needs a LiveKit deployment, so what is pinned here is everything a room hangs
//! *off*: the answers the registry gives for a session it hosts nothing for, the bounds the shipped
//! delta ring is built with, and the ref release a room performs against its checkout when it
//! closes before it ever published one.
//!
//! Real git repositories in temp directories, no LiveKit and no timers.

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_daemon::session_room::{
    delete_wip_ref, ActivityDelta, DeltaLookupError, SessionDeltaStore, SessionRoomRegistry,
    SESSION_DELTA_RING_BYTES, SESSION_DELTA_RING_TICKS,
};

/// A session on some other host, or one this daemon never opened a room for — the id every
/// accessor below is asked about.
const A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR: &str = "0199aaaa-0000-7000-8000-00000000000a";

/// A stand-in for what one poll interval of ordinary editing produces. Generous on purpose: the
/// point of using it is that a *full window* of ticks this size still fits the shipped byte bound,
/// so the two bounds are pinned as coherent with each other rather than one silently defeating the
/// other.
const AN_ORDINARY_TICKS_PATCH_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A daemon that hosts no session rooms at all — the state every daemon starts in.
fn a_registry_hosting_nothing() -> SessionRoomRegistry {
    SessionRoomRegistry::new()
}

/// The delta ring a hosted room is given, built with the bounds the daemon ships.
fn a_shipped_delta_ring() -> SessionDeltaStore {
    SessionDeltaStore::new(SESSION_DELTA_RING_TICKS, SESSION_DELTA_RING_BYTES)
}

/// One tick's delta, `patch_bytes` wide. The ring weighs a patch and never reads it, so what those
/// bytes say is not what this is about.
fn a_tick_delta(seq: u64, patch_bytes: usize) -> ActivityDelta {
    ActivityDelta {
        seq,
        prev_seq: seq.saturating_sub(1),
        base_commit: "9a1f0c2b7d4e6f8a0b1c2d3e4f5a6b7c8d9e0f11".to_string(),
        patch: vec![b'x'; patch_bytes],
        scoped_paths: Vec::new(),
    }
}

/// A checkout with one commit, standing in for a session worktree.
fn a_session_worktree(root: &Path) {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
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

/// Every ref the daemon publishes under `refs/tddy/` in `root`.
fn tddy_refs(root: &Path) -> String {
    git(root, &["for-each-ref", "--format=%(refname)", "refs/tddy/"])
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// What a daemon hosting no room answers
// ---------------------------------------------------------------------------

#[test]
fn has_no_delta_ring_for_a_session_it_hosts_no_room_for() {
    // Given a daemon hosting no session rooms
    let registry = a_registry_hosting_nothing();

    // When an RPC handler looks for that session's ring of tick deltas
    let ring = registry.delta_store(A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR);

    // Then there is none. An empty ring handed out here would answer every delta lookup with
    // "unknown call" — a defect on the client's side — where "this daemon hosts no room for that
    // session" is the answer, and the only one a caller can act on.
    assert!(
        ring.is_none(),
        "a session with no room here must have no delta ring here"
    );
}

#[test]
fn has_nowhere_to_broadcast_the_activity_of_a_session_it_hosts_no_room_for() {
    // Given a daemon hosting no session rooms
    let registry = a_registry_hosting_nothing();

    // When a reported activity record looks for the room to be published into
    let publisher = registry.activity_publisher(A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR);

    // Then there is no publisher. A record is persisted whether or not a room exists to carry it,
    // and a session whose agent runs elsewhere is the ordinary case, not a failure.
    assert!(
        publisher.is_none(),
        "a session with no room here must have no publisher here"
    );
}

#[test]
fn closing_a_session_it_hosts_no_room_for_leaves_nothing_to_serve() {
    // Given a daemon hosting no session rooms — every non-workspace session's `DeleteSession`
    let registry = a_registry_hosting_nothing();

    // When that session is deleted
    registry.close(A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR);

    // Then the registry still has neither a ring nor a publisher for it
    assert!(
        registry
            .delta_store(A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR)
            .is_none(),
        "closing an unhosted session must not leave a ring behind"
    );
    assert!(
        registry
            .activity_publisher(A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR)
            .is_none(),
        "closing an unhosted session must not leave a publisher behind"
    );
}

// ---------------------------------------------------------------------------
// What a closing room releases in the checkout
// ---------------------------------------------------------------------------

#[test]
fn releasing_a_wip_ref_a_room_never_got_as_far_as_publishing_is_not_an_error() {
    // Given a checkout whose room closed before its first tick ever published a WIP ref
    let repo = tempfile::tempdir().expect("tempdir");
    a_session_worktree(repo.path());

    // When the room closes and releases what it pinned
    let released = delete_wip_ref(repo.path(), A_SESSION_THIS_DAEMON_HOSTS_NO_ROOM_FOR);

    // Then that is not a failure. Every room takes this path when it is closed inside its first
    // poll interval, and reporting it would make an ordinary close look like a leak.
    assert_eq!(released, Ok(()));
    assert_eq!(tddy_refs(repo.path()), "");
}

// ---------------------------------------------------------------------------
// The bounds the shipped ring is built with
// ---------------------------------------------------------------------------

#[test]
fn the_shipped_delta_ring_holds_a_full_window_of_ordinary_ticks() {
    // Given the ring a hosted room is given, and a full window of ordinary ticks
    let mut ring = a_shipped_delta_ring();
    for seq in 0..SESSION_DELTA_RING_TICKS as u64 {
        ring.record(a_tick_delta(seq, AN_ORDINARY_TICKS_PATCH_BYTES));
    }

    // When the oldest of them is looked up
    let oldest = ring.residual_paths(0);

    // Then the window is retained whole. A byte bound tight enough to evict a window of ordinary
    // editing would make every client reconcile by fetching the WIP ref instead of applying the
    // deltas it is being handed — bounded, but useless.
    assert_eq!(ring.len(), SESSION_DELTA_RING_TICKS);
    assert_eq!(oldest, Ok(Vec::new()));
}

#[test]
fn the_shipped_delta_ring_evicts_the_oldest_tick_once_its_tick_bound_is_passed() {
    // Given a ring that has just been handed one tick more than its bound
    let mut ring = a_shipped_delta_ring();
    for seq in 0..=SESSION_DELTA_RING_TICKS as u64 {
        ring.record(a_tick_delta(seq, AN_ORDINARY_TICKS_PATCH_BYTES));
    }

    // When the tick that fell out is looked up
    let evicted = ring.residual_paths(0);

    // Then it aged out rather than growing the ring: a session runs for hours at the poll rate, so
    // a ring that kept every tick would be a leak measured in whole worktrees.
    assert_eq!(ring.len(), SESSION_DELTA_RING_TICKS);
    assert_eq!(
        evicted,
        Err(DeltaLookupError::AgedOut {
            call_id: String::new(),
            seq: 0,
        })
    );
}
