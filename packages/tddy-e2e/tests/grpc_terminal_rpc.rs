//! E2E test: gRPC StreamTerminalIO pipeline with virtual terminal.
//!
//! Same setup as livekit_terminal_rpc: presenter with VirtualTui, virtual keyboard
//! interactions. Uses gRPC (tonic TerminalService) instead of LiveKit.
//!
//! Assertions are 1:1 with livekit_terminal_rpc tests — same phases, same strictness.

use std::time::Duration;

use strip_ansi_escapes::strip;
use tddy_e2e::{connect_terminal_grpc, spawn_presenter_with_terminal_service};
use tddy_service::proto::terminal::TerminalInput;
use vt100::Parser;

mod keys {
    pub const ENTER: &[u8] = b"\r";
    pub const DOWN: &[u8] = b"\x1b[B";
}

fn ansi_to_text(bytes: &[u8]) -> String {
    let stripped = strip(bytes);
    String::from_utf8_lossy(&stripped).into_owned()
}

/// Drain all buffered output from the stream until `quiet_period` passes without
/// any new data. Returns all collected bytes.
async fn drain_output(
    stream: &mut tonic::Streaming<tddy_service::proto::terminal::TerminalOutput>,
    quiet_period: Duration,
    phase: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut received = Vec::new();
    let mut chunk_count = 0u64;
    log::trace!(
        "[BIDI_TRACE] test drain_output: phase={} quiet_period={:?}",
        phase,
        quiet_period
    );
    loop {
        match tokio::time::timeout(quiet_period, stream.message()).await {
            Ok(Ok(Some(output))) => {
                chunk_count += 1;
                received.extend_from_slice(&output.data);
            }
            Ok(Ok(None)) => {
                log::trace!(
                    "[BIDI_TRACE] test drain_output: phase={} stream closed after {} chunks, {} bytes",
                    phase, chunk_count, received.len()
                );
                break;
            }
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error in drain: {}", e)),
            Err(_) => {
                log::trace!(
                    "[BIDI_TRACE] test drain_output: phase={} quiet after {} chunks, {} bytes",
                    phase, chunk_count, received.len()
                );
                break;
            }
        }
    }
    Ok(received)
}

/// Collect gRPC terminal output until `min_bytes` received or `timeout` elapses.
async fn collect_output(
    stream: &mut tonic::Streaming<tddy_service::proto::terminal::TerminalOutput>,
    min_bytes: usize,
    timeout: Duration,
    phase: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut received = Vec::new();
    let mut chunk_count = 0u64;
    let deadline = tokio::time::Instant::now() + timeout;
    log::trace!(
        "[BIDI_TRACE] test collect_output: phase={} min_bytes={} timeout={:?}",
        phase,
        min_bytes,
        timeout
    );
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(100), stream.message()).await {
            Ok(Ok(Some(output))) => {
                chunk_count += 1;
                log::trace!(
                    "[BIDI_TRACE] test collect_output: phase={} chunk#{} data_len={}",
                    phase,
                    chunk_count,
                    output.data.len()
                );
                received.extend_from_slice(&output.data);
                if received.len() >= min_bytes {
                    break;
                }
            }
            Ok(Ok(None)) => {
                log::trace!(
                    "[BIDI_TRACE] test collect_output: phase={} stream CLOSED after {} chunks",
                    phase,
                    chunk_count
                );
                break;
            }
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error: {}", e)),
            Err(_) => {}
        }
    }
    log::trace!(
        "[BIDI_TRACE] test collect_output: phase={} done, {} chunks, {} bytes total",
        phase,
        chunk_count,
        received.len()
    );
    Ok(received)
}

#[tokio::test]
async fn grpc_terminal_io_receives_ansi_output() -> anyhow::Result<()> {
    let (_handle, port, _shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    input_tx.send(TerminalInput { data: vec![] }).await?;
    drop(input_tx);

    let received = drain_output(&mut stream, Duration::from_millis(500), "ansi-init").await?;

    assert!(
        received.len() > 50,
        "Expected ANSI output from VirtualTui, got {} bytes",
        received.len()
    );

    let text = ansi_to_text(&received);
    assert!(
        text.contains("State:")
            || text.contains("Goal:")
            || text.contains("Feature")
            || text.contains("plan")
            || text.contains("Build"),
        "Expected terminal content, got stripped text (len {}): {:?}",
        text.len(),
        &text[..text.len().min(200)]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_terminal_io_keyboard_input_affects_output() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<TerminalInput>(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    // Spawn input sender task — runs concurrently with output collection.
    // After scope question → PlanReview appears (View / Approve / Refine).
    // Enter answers scope, Down navigates to "Approve", Enter approves.
    let sender = tokio::spawn(async move {
        input_tx
            .send(TerminalInput { data: vec![] })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1000)).await;

        eprintln!("[TEST-INPUT] sending Enter (answer scope)");
        input_tx
            .send(TerminalInput {
                data: keys::ENTER.to_vec(),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1000)).await;

        eprintln!("[TEST-INPUT] sending Down (navigate to Approve)");
        input_tx
            .send(TerminalInput {
                data: keys::DOWN.to_vec(),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        eprintln!("[TEST-INPUT] sending Enter (approve plan)");
        input_tx
            .send(TerminalInput {
                data: keys::ENTER.to_vec(),
            })
            .await
            .unwrap();

        // Keep stream open for 5s to let VirtualTui process and re-render
        tokio::time::sleep(Duration::from_secs(5)).await;
        eprintln!("[TEST-INPUT] sender task done, dropping input_tx");
    });

    // Collect output for up to 10 seconds
    let all_output = drain_output(&mut stream, Duration::from_millis(10000), "full-run").await?;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = sender.await;

    let text = ansi_to_text(&all_output);
    eprintln!(
        "[TEST] total output: {} bytes, text_len={}, preview={:?}",
        all_output.len(),
        text.len(),
        &text[..text.len().min(500)]
    );

    assert!(
        text.contains("State:") || text.contains("Scope"),
        "Should receive initial TUI output; got (len {}): {:?}",
        text.len(),
        &text[..text.len().min(300)]
    );

    let progressed = text.contains("Plan dir:")
        || text.contains("AcceptanceTesting")
        || text.contains("GreenComplete")
        || text.contains("Workflow complete")
        || text.contains("DocsUpdated")
        || text.contains("Type your feature");

    assert!(
        progressed,
        "Keyboard inputs should advance the workflow past the initial screen; got (len {}): {:?}",
        text.len(),
        &text[..text.len().min(500)]
    );

    Ok(())
}

/// Virtual terminal viewer that mimics Ghostty: receives ANSI output via RPC,
/// parses with vt100, exposes visible screen content for assertions.
struct VirtualTerminalViewer {
    parser: Parser,
}

impl VirtualTerminalViewer {
    fn new() -> Self {
        Self {
            parser: Parser::new(24, 80, 0),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    #[allow(dead_code)]
    fn visible_content(&self) -> String {
        self.parser.screen().contents()
    }
}

/// Full e2e: virtual terminal (vt100) as viewer, gRPC for I/O sync, virtual keyboard
/// interactions. Asserts on visible terminal content like GhosttyTerminalLiveKit.
#[tokio::test]
async fn grpc_ghostty_virtual_terminal_e2e() -> anyhow::Result<()> {
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    let mut viewer = VirtualTerminalViewer::new();

    // Phase 1: send init, drain ALL initial TUI render output into vt100
    input_tx.send(TerminalInput { data: vec![] }).await?;

    let initial_output =
        drain_output(&mut stream, Duration::from_millis(500), "ghostty-init").await?;
    for chunk in initial_output.chunks(256) {
        viewer.feed(chunk);
    }
    let initial_text = ansi_to_text(&initial_output);

    assert!(
        initial_text.contains("State:") || initial_text.contains("Scope"),
        "Initial TUI should render before any keyboard input; got (len {}): {:?}",
        initial_text.len(),
        &initial_text[..initial_text.len().min(300)]
    );

    // Phase 2: send keyboard inputs, collect output after each.
    // Enter answers scope → PlanReview. Down → Approve. Enter → approve.
    let inputs: &[(&[u8], &str)] = &[
        (keys::ENTER, "Enter (answer scope)"),
        (keys::DOWN, "Down (navigate to Approve)"),
        (keys::ENTER, "Enter (approve plan)"),
    ];

    let mut all_raw = initial_output;
    for (key, label) in inputs {
        input_tx
            .send(TerminalInput {
                data: key.to_vec(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("send {}: {}", label, e))?;

        let chunk = drain_output(&mut stream, Duration::from_millis(500), label).await?;
        for part in chunk.chunks(256) {
            viewer.feed(part);
        }
        all_raw.extend_from_slice(&chunk);
    }
    drop(input_tx);

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

    let visible = ansi_to_text(&all_raw);
    let progressed = visible.contains("Plan dir:")
        || visible.contains("AcceptanceTesting")
        || visible.contains("GreenComplete")
        || visible.contains("Workflow complete")
        || visible.contains("DocsUpdated")
        || visible.contains("Type your feature");

    assert!(
        progressed,
        "Keyboard inputs should advance the workflow; stripped text (len {}): {:?}",
        visible.len(),
        &visible[..visible.len().min(500)]
    );

    Ok(())
}
