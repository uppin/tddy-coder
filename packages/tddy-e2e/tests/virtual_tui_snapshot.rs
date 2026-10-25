//! Acceptance: the coder's own TUI terminal (`TerminalServiceVirtualTui`) speaks the room wire's
//! snapshot contract — the first frame and every requested resync is a full repaint flagged as a
//! snapshot, and every frame is stamped with contiguous offsets.
//!
//! The web room feed asks for a snapshot on every open and checks offsets on every frame; a server
//! that ignored both would be exactly the silent mismatch that check exists to catch.
//! Feature doc: docs/ft/daemon/terminal-sessions.md § Room-carried terminals

use std::time::Duration;

use tddy_e2e::{
    connect_terminal_grpc, spawn_presenter_with_terminal_service, spawn_presenter_with_view_factory,
};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tddy_service::{start_virtual_tui_session, VirtualTuiSession};
use tddy_tui::VIRTUAL_TUI_REPAINT_REQUEST;
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
    assert!(
        first.data.starts_with(b"\x1bc"),
        "a snapshot begins with RIS"
    );

    // And when the client asks again mid-stream, a fresh snapshot follows deltas that were all
    // contiguous from the first one's sequence point
    tokio::time::sleep(Duration::from_millis(300)).await;
    input_tx.send(asking_for_a_snapshot()).await?;
    let mut next_start = first.end_offset;
    loop {
        let frame = next_frame(&mut stream).await;
        if frame.snapshot {
            assert!(
                frame.end_offset >= next_start,
                "a resync moves the sequence point forward"
            );
            break;
        }
        assert_eq!(
            frame.start_offset, next_start,
            "a delta starts where the last frame ended"
        );
        next_start = frame.end_offset;
    }
    Ok(())
}

/// What a VirtualTui leads a full repaint with; the terminal service sends such a frame as a
/// snapshot, prefixed with RIS.
const FULL_REPAINT_PREFIX: &[u8] = b"\x1b[2J\x1b[H";

/// The first sequence crossterm's `EnableMouseCapture` writes.
const MOUSE_TRACKING_ENABLE: &[u8] = b"\x1b[?1000h";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Reads raw VirtualTui output until a full repaint arrives, then until mouse tracking is enabled
/// again after it. Panics if either does not arrive within [`FRAME_TIMEOUT`].
async fn a_full_repaint_then_mouse_tracking(output_rx: &mut tokio::sync::mpsc::Receiver<Vec<u8>>) {
    let deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
    let mut repainted = false;
    loop {
        let chunk = match tokio::time::timeout_at(deadline, output_rx.recv()).await {
            Ok(Some(chunk)) => chunk,
            other => panic!(
                "expected {} within the timeout, got {other:?}",
                if repainted {
                    "mouse tracking re-enabled after the full repaint"
                } else {
                    "a full repaint"
                }
            ),
        };
        if chunk.starts_with(FULL_REPAINT_PREFIX) {
            repainted = true;
        }
        if repainted && contains(&chunk, MOUSE_TRACKING_ENABLE) {
            return;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_full_repaint_is_followed_by_the_mouse_tracking_its_reset_undid() -> anyhow::Result<()>
{
    // Given a mouse-enabled VirtualTui that has painted its first screen
    let (_handle, factory, _shutdown) = spawn_presenter_with_view_factory(None);
    let Some(VirtualTuiSession {
        input_tx,
        mut output_rx,
        shutdown,
    }) = start_virtual_tui_session(&*factory, true)
    else {
        anyhow::bail!("connect_view / start_virtual_tui_session");
    };
    a_full_repaint_then_mouse_tracking(&mut output_rx).await;

    // When the client resizes it, then (separately) asks it for a repaint
    input_tx.send(b"\x1b]resize;100;30\x07".to_vec()).await?;

    // Then each full repaint, which reaches the client led by RIS, is followed by mouse tracking
    a_full_repaint_then_mouse_tracking(&mut output_rx).await;
    input_tx.send(VIRTUAL_TUI_REPAINT_REQUEST.to_vec()).await?;
    a_full_repaint_then_mouse_tracking(&mut output_rx).await;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
