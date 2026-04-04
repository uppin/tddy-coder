//! Acceptance tests: gRPC terminal reconnect must receive a full TUI render on the second
//! `stream_terminal_io` attachment (same running presenter). See PRD Testing Plan.
//!
//! Uses view-local selection (Select mode): after moving highlight to **No** on the
//! acceptance-tests permission prompt, a new stream must rebuild TuiView from authoritative
//! presenter/view sync so the **same** option stays selected — a plain `PresenterState`
//! snapshot without view replay is insufficient (PRD).
//!
//! (Plan-phase clarify is skipped when the interview handoff supplies answers — stub treats that
//! as `HERE ARE THE USER'S ANSWERS` — so we assert on the next Select: acceptance-tests permission.)
//!
//! PRD `virtual_tui_attach_forces_full_frame_once` (clear/home + full composited frame on new
//! attach) is covered by the reconnect burst assertions in
//! `grpc_reconnect_second_stream_receives_full_tui_render` before the view-state checks.

use std::time::Duration;

use strip_ansi_escapes::strip;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use tddy_e2e::{connect_grpc, connect_terminal_grpc, spawn_presenter_with_terminal_service};
use tddy_service::gen::app_mode_proto;
use tddy_service::gen::server_message;
use tddy_service::gen::{client_message, ApproveSessionDocument, ClientMessage};
use tddy_service::proto::terminal::TerminalInput;
use tddy_tui_testkit::ScreenParser;

mod keys {
    pub const DOWN: &[u8] = b"\x1b[B";
}

fn ansi_to_text(bytes: &[u8]) -> String {
    let stripped = strip(bytes);
    String::from_utf8_lossy(&stripped).into_owned()
}

async fn collect_output_window(
    stream: &mut tonic::Streaming<tddy_service::proto::terminal::TerminalOutput>,
    window: Duration,
) -> anyhow::Result<Vec<u8>> {
    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let step = remaining.min(Duration::from_millis(50));
        match tokio::time::timeout(step, stream.message()).await {
            Ok(Ok(Some(output))) => received.extend_from_slice(&output.data),
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error: {}", e)),
            Err(_) => {}
        }
    }
    Ok(received)
}

/// Acceptance: second `stream_terminal_io` must emit a client-syncing full frame (clear/home
/// prefix) and preserve visible Select-mode highlight across reconnect (PRD: view state).
/// Select highlight is synced to the presenter via `UserIntent::SelectHighlightChanged` so
/// `connect_view` snapshots include the current option.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_reconnect_second_stream_receives_full_tui_render() -> anyhow::Result<()> {
    std::env::set_var("TDDY_DISABLE_ANIMATIONS", "1");
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    // PRD approval is a gRPC `DocumentReview` mode; approve so the stub run reaches acceptance-tests.
    let port_doc = port;
    let _doc_approve = tokio::spawn(async move {
        let Ok(mut grpc_client) = connect_grpc(port_doc).await else {
            return;
        };
        let (tx, rx) = tokio::sync::mpsc::channel::<ClientMessage>(8);
        let request_stream = ReceiverStream::new(rx);
        let Ok(resp) = grpc_client.stream(Request::new(request_stream)).await else {
            return;
        };
        let mut stream = resp.into_inner();
        for _ in 0..600 {
            match tokio::time::timeout(Duration::from_millis(50), stream.message()).await {
                Ok(Ok(Some(msg))) => {
                    if let Some(server_message::Event::ModeChanged(mc)) = msg.event {
                        if let Some(mode) = &mc.mode {
                            if let Some(app_mode_proto::Variant::DocumentReview(_)) = &mode.variant
                            {
                                let _ = tx
                                    .send(ClientMessage {
                                        intent: Some(
                                            client_message::Intent::ApproveSessionDocument(
                                                ApproveSessionDocument {},
                                            ),
                                        ),
                                    })
                                    .await;
                                break;
                            }
                        }
                    }
                }
                Ok(Ok(None)) => break,
                _ => {}
            }
        }
    });

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx1, input_rx1) = tokio::sync::mpsc::channel::<TerminalInput>(64);
    let input_stream1 = tokio_stream::wrappers::ReceiverStream::new(input_rx1);

    let mut stream1 = client
        .stream_terminal_io(tonic::Request::new(input_stream1))
        .await?
        .into_inner();

    input_tx1.send(TerminalInput { data: vec![] }).await?;
    let mut stream1_output = Vec::new();
    for _ in 0..50 {
        let chunk = collect_output_window(&mut stream1, Duration::from_millis(200)).await?;
        stream1_output.extend_from_slice(&chunk);
        let t = ansi_to_text(&stream1_output);
        if t.contains("Permission") && t.contains("> Yes") {
            break;
        }
    }
    let initial_text = ansi_to_text(&stream1_output);
    assert!(
        initial_text.contains("Permission") || initial_text.contains("Allow creating"),
        "Should reach acceptance-tests permission Select; got: {:?}",
        &initial_text[..initial_text.len().min(400)]
    );

    let mut parser1 = ScreenParser::new(24, 80);
    parser1.feed(&stream1_output);
    let before = parser1.contents();
    assert!(
        before.contains("> Yes"),
        "Initially first option should be selected; screen:\n{}",
        before
    );

    input_tx1
        .send(TerminalInput {
            data: keys::DOWN.to_vec(),
        })
        .await?;
    let after_down = collect_output_window(&mut stream1, Duration::from_millis(900)).await?;
    let mut full_out = stream1_output;
    full_out.extend_from_slice(&after_down);

    let mut parser_after = ScreenParser::new(24, 80);
    parser_after.feed(&full_out);
    let reference_screen = parser_after.contents();
    assert!(
        reference_screen.contains("> No"),
        "Stream1: Down should select No; screen:\n{}",
        reference_screen
    );
    assert!(
        !reference_screen.contains("> Yes"),
        "Stream1: first option must not stay selected after Down; screen:\n{}",
        reference_screen
    );

    drop(input_tx1);
    drop(stream1);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (input_tx2, input_rx2) = tokio::sync::mpsc::channel::<TerminalInput>(64);
    let input_stream2 = tokio_stream::wrappers::ReceiverStream::new(input_rx2);

    let mut stream2 = client
        .stream_terminal_io(tonic::Request::new(input_stream2))
        .await?
        .into_inner();

    let reconnect_burst = collect_output_window(&mut stream2, Duration::from_millis(2000)).await?;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx2);

    const CLEAR_HOME: &[u8] = b"\x1b[2J\x1b[H";
    assert!(
        reconnect_burst.starts_with(CLEAR_HOME),
        "reconnect must begin with full-screen clear + home so empty VT clients resync; \
         prefix {:?}",
        &reconnect_burst[..reconnect_burst.len().min(16)]
    );

    // Full composited frame size varies slightly (e.g. idle status dot vs fast spinner ANSI churn).
    const MIN_RECONNECT_FRAME_BYTES: usize = 500;
    assert!(
        reconnect_burst.len() >= MIN_RECONNECT_FRAME_BYTES,
        "reconnect initial output must be a substantial composited frame (>= {} bytes), got {}",
        MIN_RECONNECT_FRAME_BYTES,
        reconnect_burst.len()
    );

    let mut p2 = ScreenParser::new(24, 80);
    p2.feed(&reconnect_burst);
    let reconnect_screen = p2.contents();

    assert!(
        reconnect_screen.contains("> No"),
        "Reconnect must preserve Select highlight on No (view-local state + presenter sync). \
         Got screen:\n{}",
        reconnect_screen
    );
    assert!(
        !reconnect_screen.contains("> Yes"),
        "Reconnect must not reset selection to the first option. Screen:\n{}",
        reconnect_screen
    );

    Ok(())
}
