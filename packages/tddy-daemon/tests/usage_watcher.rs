//! Integration tests: the per-session usage emitter that the file-watcher drives.
//!
//! The watcher re-reads the on-disk token sources on start and on every change, gathers them
//! (`tddy_core::backend::gather_session_usage`), and hands the snapshot to a `SessionUsageEmitter`.
//! The emitter broadcasts a `PresenterEvent::TokenUsageUpdated` carrying the full cumulative
//! snapshot — always on the first gather (so a newly-connected Inspector sees current totals), and
//! again whenever the snapshot changes, but never for an unchanged snapshot.
//!
//! Gathering-from-disk (including "an appended transcript yields larger totals") is covered by the
//! `tddy-core` `gather_session_usage` tests; these pin the emitter's broadcast/dedup contract.

use tddy_core::token_accounting::ConversationRecord;
use tddy_core::PresenterEvent;
use tddy_daemon::usage_watcher::SessionUsageEmitter;
use tokio::sync::broadcast;

fn a_record(id: &str, input: u64, output: u64) -> ConversationRecord {
    ConversationRecord {
        agent: "claude".to_string(),
        id: id.to_string(),
        model: "claude-opus-4-8".to_string(),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output,
        turns: 1,
    }
}

fn recv_snapshot(rx: &mut broadcast::Receiver<PresenterEvent>) -> Vec<ConversationRecord> {
    match rx.try_recv() {
        Ok(PresenterEvent::TokenUsageUpdated(records)) => records,
        Ok(other) => panic!("expected a TokenUsageUpdated event, got {other:?}"),
        Err(e) => panic!("expected a broadcast TokenUsageUpdated event, got {e:?}"),
    }
}

#[test]
fn emits_the_current_snapshot_the_first_time_the_watcher_gathers() {
    // Given a subscribed emitter
    let (tx, mut rx) = broadcast::channel(16);
    let mut emitter = SessionUsageEmitter::new(tx);

    // When the watcher gathers usage for the first time
    let emitted = emitter.emit_if_changed(vec![a_record("claude-main", 100, 20)]);

    // Then a snapshot is broadcast carrying the current totals
    assert!(emitted, "the first gather must always broadcast a snapshot");
    assert_eq!(
        recv_snapshot(&mut rx),
        vec![a_record("claude-main", 100, 20)]
    );
}

#[test]
fn re_emits_the_new_snapshot_after_usage_grows() {
    // Given an emitter that has already broadcast the initial snapshot
    let (tx, mut rx) = broadcast::channel(16);
    let mut emitter = SessionUsageEmitter::new(tx);
    emitter.emit_if_changed(vec![a_record("claude-main", 100, 20)]);
    let _ = rx.try_recv();

    // When a later gather reports more tokens for the main agent
    let emitted = emitter.emit_if_changed(vec![a_record("claude-main", 150, 25)]);

    // Then the new snapshot is broadcast
    assert!(emitted, "a changed snapshot must be re-broadcast");
    assert_eq!(
        recv_snapshot(&mut rx),
        vec![a_record("claude-main", 150, 25)]
    );
}

#[test]
fn does_not_re_emit_an_unchanged_snapshot() {
    // Given an emitter that has already broadcast a snapshot
    let (tx, mut rx) = broadcast::channel(16);
    let mut emitter = SessionUsageEmitter::new(tx);
    emitter.emit_if_changed(vec![a_record("claude-main", 100, 20)]);
    let _ = rx.try_recv();

    // When the next gather is identical
    let emitted = emitter.emit_if_changed(vec![a_record("claude-main", 100, 20)]);

    // Then nothing is broadcast
    assert!(!emitted, "an unchanged snapshot must not be re-broadcast");
    assert!(
        rx.try_recv().is_err(),
        "no event should be broadcast for an unchanged snapshot"
    );
}
