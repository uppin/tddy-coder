//! Acceptance test: a second gRPC terminal client connecting with smaller screen
//! dimensions must not blank or alter the first client's rendered TUI.
//!
//! Bug: reconnecting to a session in tddy-web results in a blank terminal (only cursor
//! blinking). The second RPC TUI session somehow affects the first one's output.

use std::time::Duration;

use tddy_e2e::rpc_frontend::encode_resize;
use tddy_e2e::{connect_terminal_grpc, spawn_presenter_with_terminal_service};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tddy_tui_testkit::ScreenParser;

/// Idle status-bar pulse cycles `·` / `•` / `●` (see `IDLE_DOT_PULSE_CHARS` in tddy-tui). Two snapshots
/// taken a second apart may differ only by that glyph; normalize so we still detect blanking regressions.
fn normalize_idle_pulse(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '•' | '●' => '·',
            c => c,
        })
        .collect()
}

async fn collect_output_window(
    stream: &mut tonic::Streaming<TerminalOutput>,
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

/// Second gRPC client with smaller screen must not blank the first client's TUI.
///
/// 1. Client 1 connects at 80×24 (default), waits for user question to appear.
/// 2. Client 2 connects and resizes to 60×16.
/// 3. Client 2 must also show the user question at its smaller dimensions.
/// 4. Client 1's accumulated screen must still show the user question — not go blank.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn second_client_smaller_resize_does_not_blank_first_client() -> anyhow::Result<()> {
    std::env::set_var("TDDY_DISABLE_ANIMATIONS", "1");
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    // --- Client 1: connect at default 80×24 ---
    let mut client1 = connect_terminal_grpc(port).await?;
    let (input_tx1, input_rx1) = tokio::sync::mpsc::channel::<TerminalInput>(64);
    let input_stream1 = tokio_stream::wrappers::ReceiverStream::new(input_rx1);
    let mut stream1 = client1
        .stream_terminal_io(tonic::Request::new(input_stream1))
        .await?
        .into_inner();

    input_tx1.send(TerminalInput { data: vec![] }).await?;
    let stream1_initial = collect_output_window(&mut stream1, Duration::from_millis(700)).await?;

    let mut parser1 = ScreenParser::new(24, 80);
    parser1.feed(&stream1_initial);
    let screen1_before = parser1.contents();
    assert!(
        screen1_before.contains("Email/password") || screen1_before.contains("Scope"),
        "Client 1 should see user question at 80×24; got:\n{}",
        screen1_before
    );

    // --- Client 2: connect and resize to 60×16 ---
    let mut client2 = connect_terminal_grpc(port).await?;
    let (input_tx2, input_rx2) = tokio::sync::mpsc::channel::<TerminalInput>(64);
    let input_stream2 = tokio_stream::wrappers::ReceiverStream::new(input_rx2);
    let mut stream2 = client2
        .stream_terminal_io(tonic::Request::new(input_stream2))
        .await?
        .into_inner();

    input_tx2
        .send(TerminalInput {
            data: encode_resize(60, 16),
        })
        .await?;
    let stream2_output = collect_output_window(&mut stream2, Duration::from_millis(1500)).await?;

    let mut parser2 = ScreenParser::new(16, 60);
    parser2.feed(&stream2_output);
    let screen2 = parser2.contents();
    assert!(
        screen2.contains("Email/password") || screen2.contains("Scope"),
        "Client 2 (60×16) should see the user question; got:\n{}",
        screen2
    );

    // --- Verify client 1 was not affected ---
    let stream1_after_client2 =
        collect_output_window(&mut stream1, Duration::from_millis(1500)).await?;
    parser1.feed(&stream1_after_client2);
    let screen1_after = parser1.contents();

    assert!(
        screen1_after.contains("Email/password") || screen1_after.contains("Scope"),
        "Client 1 must not go blank after client 2 connects with smaller screen; got:\n{}",
        screen1_after
    );

    assert_eq!(
        normalize_idle_pulse(&screen1_before),
        normalize_idle_pulse(&screen1_after),
        "Client 1 screen content must not change when client 2 connects with different dimensions.\n\
         Before:\n{}\n\nAfter:\n{}",
        screen1_before, screen1_after
    );

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx1);
    drop(input_tx2);

    Ok(())
}
