//! Acceptance: the capture lock is a sequence point between a terminal's screen and its live output.
//!
//! A reconnecting client is handed a snapshot and then the live stream. If a byte can be appended to
//! the capture without being broadcast under the same lock — or broadcast before it is appended — a
//! reader that snapshots and subscribes under that lock either misses the byte (a hole on screen) or
//! receives it twice (a duplicated draw). The race only shows under concurrent output, so this
//! test runs it many times with a producer that never pauses.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bytes::Bytes;
use tddy_task::{ChannelKind, TaskChannel};

/// Enough chunks that the reader's snapshot lands mid-stream; fewer than the broadcast's capacity,
/// so a lagging receiver cannot be what the test measures.
const CHUNKS: usize = 200;
const RUNS: usize = 200;

/// The bytes the producer writes, numbered so a duplicate or a gap is visible in a failure message.
fn chunk(n: usize) -> Vec<u8> {
    format!("<{n:04}>").into_bytes()
}

fn everything_the_producer_writes() -> Vec<u8> {
    (0..CHUNKS).flat_map(chunk).collect()
}

/// Snapshot the capture and subscribe to live output under one lock, as a reconnecting stream does.
fn snapshot_offset_and_subscribe(
    channel: &TaskChannel,
) -> (u64, tokio::sync::broadcast::Receiver<Bytes>) {
    let capture = channel.capture_arc();
    let held = capture.lock().unwrap();
    let snapshot = held.snapshot().expect("the capture was sized");
    let live = channel.subscribe();
    drop(held);
    (snapshot.offset, live)
}

/// Every live byte the receiver gets until it has `expected` of them.
fn receive(mut live: tokio::sync::broadcast::Receiver<Bytes>, expected: usize) -> Vec<u8> {
    let mut received = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while received.len() < expected {
        assert!(
            std::time::Instant::now() < deadline,
            "received {} of {expected} live bytes before the deadline",
            received.len()
        );
        match live.try_recv() {
            Ok(bytes) => received.extend_from_slice(&bytes),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => thread::yield_now(),
            Err(other) => panic!("the live stream failed: {other:?}"),
        }
    }
    received
}

#[test]
fn snapshot_and_subscribe_neither_loses_nor_duplicates_a_byte_under_concurrent_output() {
    let stream = everything_the_producer_writes();
    let mut snapshots_taken_mid_stream = 0;

    for run in 0..RUNS {
        // Given a terminal whose application is writing output without pause
        let channel = TaskChannel::output_only("0", "pty", ChannelKind::Pty);
        channel.capture_arc().lock().unwrap().resize(24, 80);
        let producer = {
            let channel = Arc::clone(&channel);
            thread::spawn(move || {
                for n in 0..CHUNKS {
                    channel.write(Bytes::from(chunk(n)));
                }
            })
        };

        // When a client snapshots and subscribes once the output is under way
        while channel.capture_arc().lock().unwrap().end_offset() < (stream.len() / 4) as u64 {
            thread::yield_now();
        }
        let (offset, live) = snapshot_offset_and_subscribe(&channel);
        if offset < stream.len() as u64 {
            snapshots_taken_mid_stream += 1;
        }
        let expected = stream.len() - offset as usize;
        let received = receive(live, expected);
        producer.join().unwrap();

        // Then the live stream is exactly the output from the snapshot's sequence point on
        assert_eq!(
            String::from_utf8_lossy(&received),
            String::from_utf8_lossy(&stream[offset as usize..]),
            "run {run}: live bytes after a snapshot at offset {offset}"
        );
    }

    // A run whose snapshot came after the producer finished proves nothing about the race.
    assert!(
        snapshots_taken_mid_stream > 0,
        "no run took its snapshot while output was still being written"
    );
}
