//! Rendering a worktree-activity broadcast for the log.
//!
//! Receiving an event on the `worktree.activity` topic does exactly one thing today: emit a single
//! `DEBUG` line. That line is the whole observable behaviour, so it is written once here and every
//! receiver calls it rather than each inventing its own phrasing.
//!
//! It lives in `tddy-service` rather than `tddy-daemon` because the daemon is not the only
//! consumer: `tddy-tools` logs the same events in a split agent's process, and it depends on
//! `tddy-service` unconditionally while its `tddy-livekit` dependency is feature-gated.

use crate::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};

/// The data-channel topic worktree activity is broadcast on. Deliberately not `tddy-rpc`: every RPC
/// receiver in the system hard-filters on that topic, so an activity event published there would be
/// dropped by some peers and mistaken for a request by others.
///
/// Beside the payload's schema and its log rendering, because publisher and receiver live in
/// different crates: a topic each of them spelled for itself would fail as silence — every receiver
/// filters by topic, so a mismatch delivers nothing and reports nothing.
pub const WORKTREE_ACTIVITY_TOPIC: &str = "worktree.activity";

/// How much of a commit sha the log line carries. A full 40 characters in every line of a busy log
/// buys nothing a reader can use — seven is enough to recognise a commit and to paste into `git
/// show`.
const SHORT_SHA_LEN: usize = 7;

/// Render one received activity event as the single `DEBUG` line its receiver emits.
///
/// A kind this build does not recognise still produces a line naming its raw wire value: an event
/// published by a newer daemon is worth knowing about even when this one cannot say what it means.
pub fn format_worktree_activity_for_log(event: &WorktreeActivityEvent) -> String {
    match event.kind() {
        WorktreeActivityKind::Commit => format!(
            "worktree activity: commit seq={} head={}",
            event.seq,
            short_sha(&event.head_commit)
        ),
        WorktreeActivityKind::FilesChanged => format!(
            "worktree activity: files_changed seq={} files={} +{} -{}",
            event.seq, event.changed_files, event.lines_added, event.lines_removed
        ),
        WorktreeActivityKind::Unspecified => format!(
            "worktree activity: unrecognized kind={} seq={}",
            event.kind, event.seq
        ),
    }
}

/// Abbreviate a commit sha, tolerating one shorter than [`SHORT_SHA_LEN`] rather than slicing past
/// its end. Counting characters keeps this total for any string the wire happens to carry.
fn short_sha(head_commit: &str) -> String {
    head_commit.chars().take(SHORT_SHA_LEN).collect()
}
