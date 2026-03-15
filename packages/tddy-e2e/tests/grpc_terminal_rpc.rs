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

/// Bug reproduction: VirtualTui only renders when a PresenterEvent or keyboard input
/// arrives. The status-bar spinner (cycling |, /, -, \) and elapsed timer freeze
/// between events because the render loop gates on `if updated { render(); }`.
///
/// The real TUI event loop (event_loop.rs) renders every ~50ms unconditionally,
/// keeping the spinner alive. The VirtualTui should do the same — periodic re-renders
/// even when idle.
///
/// Setup: connect gRPC terminal, let the workflow reach Select mode (waiting for user
/// input, no more PresenterEvents), then verify output CONTINUES to arrive without
/// any keyboard input — proving the TUI is re-rendering autonomously.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_virtual_tui_refreshes_autonomously_without_input() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    // Send init to start the stream.
    input_tx.send(TerminalInput { data: vec![] }).await?;

    // Drain the initial event burst. The workflow starts, emits GoalStarted,
    // StateChanged, ModeChanged etc., then reaches Select mode (scope question)
    // and stops emitting events. A 500ms quiet period is plenty for StubBackend.
    let initial = drain_output(&mut stream, Duration::from_millis(500), "init-burst").await?;
    let initial_text = ansi_to_text(&initial);
    assert!(
        initial.len() > 50,
        "Should receive initial TUI render, got {} bytes",
        initial.len()
    );
    eprintln!(
        "[TEST] initial burst: {} bytes, text preview: {:?}",
        initial.len(),
        &initial_text[..initial_text.len().min(200)]
    );

    // Now: presenter is in Select mode, waiting for user input.
    // No PresenterEvents will arrive. No keyboard input is sent.
    // The VirtualTui SHOULD still re-render periodically — the status-bar
    // spinner cycles through |, /, -, \ and the elapsed timer ticks.
    //
    // With the bug, `updated` is never set to true, render() is never called,
    // and zero bytes are emitted. The spinner and timer freeze.
    let autonomous_output = collect_output(
        &mut stream,
        1, // min_bytes: we just need ANY output to prove re-rendering
        Duration::from_secs(2),
        "autonomous-refresh",
    )
    .await?;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx);

    assert!(
        !autonomous_output.is_empty(),
        "VirtualTui should re-render autonomously (spinner tick, elapsed timer) \
         even without PresenterEvents or keyboard input, but received 0 bytes \
         in 2 seconds. The render loop only triggers on events/input — \
         it needs a periodic re-render like the real TUI event loop (every ~50ms)."
    );

    Ok(())
}

/// Bug reproduction: in Select mode over RPC, pressing Down arrow briefly moves the
/// selection highlight but the periodic re-render resets it back to the first option.
///
/// The user sees the selection "blink" on the next option then snap back to the first.
/// This happens because the periodic render (200ms tick) somehow overwrites the
/// view-local `select_selected` state, or the frame sent to the client restores the
/// old selection.
///
/// This test sends a Down arrow during Select mode, waits for several periodic render
/// ticks, then feeds all received output into a vt100 parser to read the final visible
/// screen. The second option ("OAuth") should have the selection indicator "> ", not
/// the first option ("Email/password").
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_select_mode_down_arrow_persists_after_periodic_render() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    // Send init, drain the initial burst until Select mode is reached.
    input_tx.send(TerminalInput { data: vec![] }).await?;
    let initial = drain_output(&mut stream, Duration::from_millis(500), "init").await?;
    let initial_text = ansi_to_text(&initial);
    assert!(
        initial_text.contains("Email/password") || initial_text.contains("Scope"),
        "Should reach Select mode with authentication question; got: {:?}",
        &initial_text[..initial_text.len().min(300)]
    );

    // Feed initial output into vt100 parser to verify initial state:
    // first option "Email/password" should have "> " prefix (selected).
    let mut parser = Parser::new(24, 80, 0);
    parser.process(&initial);
    let before_screen = parser.screen().contents();
    eprintln!("[TEST] before Down — screen:\n{}", before_screen);

    // Verify initial selection is on first option.
    assert!(
        before_screen.contains("> Email/password"),
        "Initially the first option should be selected with '> ' prefix; screen:\n{}",
        before_screen
    );

    // Send Down arrow to move selection to second option ("OAuth").
    input_tx
        .send(TerminalInput {
            data: keys::DOWN.to_vec(),
        })
        .await?;

    // Wait long enough for several periodic render ticks (200ms each).
    // Collect ALL output chunks individually and check each one for selection state.
    // The bug: selection briefly shows on the correct option then resets. We need to
    // verify that EVERY chunk after Down maintains the correct selection, not just the
    // final state.
    let mut chunks_with_selection_reset = Vec::new();
    let mut chunk_parser = Parser::new(24, 80, 0);
    chunk_parser.process(&initial); // start from same state
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut chunk_idx = 0u32;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(300), stream.message()).await {
            Ok(Ok(Some(output))) => {
                chunk_idx += 1;
                chunk_parser.process(&output.data);
                let screen = chunk_parser.screen().contents();
                if screen.contains("> Email/password") && !screen.contains("> OAuth") {
                    chunks_with_selection_reset.push((chunk_idx, screen.clone()));
                }
                parser.process(&output.data);
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error: {}", e)),
            Err(_) => break,
        }
    }
    let after_screen = parser.screen().contents();
    eprintln!("[TEST] after Down + periodic renders — screen:\n{}", after_screen);
    if !chunks_with_selection_reset.is_empty() {
        eprintln!(
            "[TEST] WARNING: {} chunks showed selection reset to first option!",
            chunks_with_selection_reset.len()
        );
        for (idx, screen) in &chunks_with_selection_reset {
            eprintln!("[TEST] chunk #{} had selection reset:\n{}", idx, screen);
        }
    }

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx);

    // The selection should have PERSISTED on the second option ("OAuth") across
    // all periodic render ticks. If the bug exists, the selection resets to
    // "Email/password" after the initial blink.
    assert!(
        after_screen.contains("> OAuth"),
        "After pressing Down, the selection should persist on 'OAuth' across periodic renders. \
         The selection was reset back to the first option. Screen:\n{}",
        after_screen
    );
    assert!(
        !after_screen.contains("> Email/password"),
        "The first option should NOT have the selection indicator after pressing Down. Screen:\n{}",
        after_screen
    );

    // Verify NO intermediate chunks showed the selection resetting (the "blink" bug).
    assert!(
        chunks_with_selection_reset.is_empty(),
        "Selection should never reset to the first option after Down arrow was processed. \
         {} out of {} chunks showed the selection back on 'Email/password' (the blink bug).",
        chunks_with_selection_reset.len(),
        chunk_idx
    );

    Ok(())
}
