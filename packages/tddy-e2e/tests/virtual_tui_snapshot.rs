//! Acceptance: the coder's own TUI terminal (`TerminalServiceVirtualTui`) speaks the room wire's
//! snapshot contract — the first frame and every requested resync is a full repaint flagged as a
//! snapshot, and every frame is stamped with contiguous offsets.
//!
//! The web room feed asks for a snapshot on every open and checks offsets on every frame; a server
//! that ignored both would be exactly the silent mismatch that check exists to catch.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-room.md

use std::time::Duration;

use tddy_e2e::{connect_terminal_grpc, spawn_presenter_with_terminal_service};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tokio_stream::StreamExt;

const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

fn asking_for_a_snapshot() -> TerminalInput {
    TerminalInput {
        request_snapshot: true,
        ..Default::default()
    }
}

async fn next_frame(stream: &mut tonic::Streaming<TerminalOutput>) -> TerminalOutput {
    match tokio::time::timeout(FRAME_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(frame))) => frame,
        other => panic!("expected a terminal frame, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_frame_and_every_requested_resync_is_a_snapshot_with_contiguous_offsets(
) -> anyhow::Result<()> {
    // Given the coder's TUI served on the room wire
    let (_handle, port, _shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));
    let mut client = connect_terminal_grpc(port).await?;
    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let mut stream = client
        .stream_terminal_io(tonic::Request::new(
            tokio_stream::wrappers::ReceiverStream::new(input_rx),
        ))
        .await?
        .into_inner();

    // When a client opens it asking for a snapshot
    input_tx.send(asking_for_a_snapshot()).await?;
    let first = next_frame(&mut stream).await;

    // Then the first frame is a full repaint flagged as a snapshot
    assert!(first.snapshot, "the first frame is a snapshot");
    assert!(first.data.starts_with(b"\x1bc"), "a snapshot begins with RIS");

    // And when the client asks again mid-stream, a fresh snapshot follows deltas that were all
    // contiguous from the first one's sequence point
    tokio::time::sleep(Duration::from_millis(300)).await;
    input_tx.send(asking_for_a_snapshot()).await?;
    let mut next_start = first.end_offset;
    loop {
        let frame = next_frame(&mut stream).await;
        if frame.snapshot {
            assert!(frame.end_offset >= next_start, "a resync moves the sequence point forward");
            break;
        }
        assert_eq!(frame.start_offset, next_start, "a delta starts where the last frame ended");
        next_start = frame.end_offset;
    }
    Ok(())
}
