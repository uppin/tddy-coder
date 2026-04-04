//! E2E test: gRPC StreamTerminalIO pipeline with virtual terminal.
//!
//! Same setup as livekit_terminal_rpc: presenter with VirtualTui, virtual keyboard
//! interactions. Uses gRPC (tonic TerminalService) instead of LiveKit.
//!
//! Assertions are 1:1 with livekit_terminal_rpc tests — same phases, same strictness.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::sync::atomic::Ordering;
use strip_ansi_escapes::strip;

use tddy_e2e::rpc_frontend::encode_resize;
use tddy_e2e::{
    connect_terminal_grpc, spawn_presenter_with_terminal_service, spawn_presenter_with_view_factory,
};
use tddy_service::proto::terminal::TerminalInput;
use tddy_service::{start_virtual_tui_session, VirtualTuiSession};
use tddy_tui_testkit::{
    assert_segmented_echo, eventually_segmented_echo, ScreenParser, SegmentedEchoFailureStyle,
    SegmentedEchoWaitParams,
};

mod keys {
    pub const ENTER: &[u8] = b"\r";
    pub const DOWN: &[u8] = b"\x1b[B";
}

/// Prefix for assertion/debug output without splitting a UTF-8 codepoint.
fn utf8_preview(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

static LARGE_ECHO_TEST_LOCK: Mutex<()> = Mutex::new(());

const LARGE_ECHO_CHAR_CAP: usize = 1_000;
const LARGE_ECHO_SEGMENTS: usize = 10;

/// Wait for VirtualTui / RPC to catch up: input can be queued ahead of rendered output.
const LARGE_ECHO_VT100_SYNC_TIMEOUT: Duration = Duration::from_secs(120);
/// Full VT100 parse + binary search each iteration is expensive; throttle checks.
const LARGE_ECHO_VT100_SYNC_MIN_INTERVAL: Duration = Duration::from_millis(400);
const LARGE_ECHO_VT100_SYNC_MIN_NEW_BYTES: usize = 4096;
const LARGE_ECHO_VT100_SYNC_LOOP_SLEEP: Duration = Duration::from_millis(50);

/// Builds a single feature string of exactly `total_len` Unicode scalars. Each segment starts
/// with `#SEG-<i>:` followed by `a` padding so the total length matches.
fn build_large_echo_segmented_payload(
    total_len: usize,
    num_segments: usize,
) -> (String, Vec<String>) {
    assert!(num_segments > 0);
    let headers: Vec<String> = (0..num_segments).map(|i| format!("#SEG-{}:", i)).collect();
    let header_chars: usize = headers.iter().map(|s| s.chars().count()).sum();
    assert!(
        header_chars <= total_len,
        "segment headers exceed total_len={} (headers use {} chars, {} segments)",
        total_len,
        header_chars,
        num_segments
    );
    let body_total = total_len - header_chars;
    let base = body_total / num_segments;
    let rem = body_total % num_segments;
    let mut segments: Vec<String> = Vec::with_capacity(num_segments);
    for (i, header) in headers.iter().enumerate() {
        let body_len = base + if i < rem { 1 } else { 0 };
        let mut seg = header.clone();
        seg.extend(std::iter::repeat_n('a', body_len));
        segments.push(seg);
    }
    let full: String = segments.iter().cloned().collect();
    assert_eq!(full.chars().count(), total_len);
    (full, segments)
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
                    phase,
                    chunk_count,
                    received.len()
                );
                break;
            }
        }
    }
    Ok(received)
}

/// Collect gRPC terminal output until `min_bytes` received or `timeout` elapses.
#[allow(dead_code)] // Used by some scenarios; idle-cadence tests use [`count_terminal_chunks_in_window`].
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

/// Count gRPC terminal output messages received during a wall-clock window (no minimum byte threshold).
async fn count_terminal_chunks_in_window(
    stream: &mut tonic::Streaming<tddy_service::proto::terminal::TerminalOutput>,
    window: Duration,
    phase: &str,
) -> anyhow::Result<usize> {
    let mut n = 0usize;
    let deadline = tokio::time::Instant::now() + window;
    log::trace!(
        "[BIDI_TRACE] count_terminal_chunks_in_window: phase={} window={:?}",
        phase,
        window
    );
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(50), stream.message()).await {
            Ok(Ok(Some(_))) => n += 1,
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error in {}: {}", phase, e)),
            Err(_) => {}
        }
    }
    log::trace!(
        "[BIDI_TRACE] count_terminal_chunks_in_window: phase={} done chunks={}",
        phase,
        n
    );
    Ok(n)
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
        utf8_preview(&text, 200)
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
        input_tx.send(TerminalInput { data: vec![] }).await.unwrap();
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
        utf8_preview(&text, 500)
    );

    assert!(
        text.contains("State:") || text.contains("Scope"),
        "Should receive initial TUI output; got (len {}): {:?}",
        text.len(),
        utf8_preview(&text, 300)
    );

    let progressed = text.contains("Session dir:")
        || text.contains("AcceptanceTesting")
        || text.contains("GreenComplete")
        || text.contains("Workflow complete")
        || text.contains("DocsUpdated")
        || text.contains("Type your feature")
        || text.contains("Planning→Planned");

    assert!(
        progressed,
        "Keyboard inputs should advance the workflow past the initial screen; got (len {}): {:?}",
        text.len(),
        utf8_preview(&text, 500)
    );

    Ok(())
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

    let mut viewer = ScreenParser::new(24, 80);

    // Phase 1: send init, drain ALL initial TUI render output into vt100
    input_tx.send(TerminalInput { data: vec![] }).await?;

    let initial_output =
        drain_output(&mut stream, Duration::from_millis(500), "ghostty-init").await?;
    for chunk in initial_output.chunks(256) {
        viewer.feed(chunk);
    }
    let initial_text = ansi_to_text(&initial_output);

    let preview_300: String = initial_text.chars().take(300).collect();
    assert!(
        initial_text.contains("State:") || initial_text.contains("Scope"),
        "Initial TUI should render before any keyboard input; got (len {}): {:?}",
        initial_text.len(),
        preview_300
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
            .send(TerminalInput { data: key.to_vec() })
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
    let progressed = visible.contains("Session dir:")
        || visible.contains("AcceptanceTesting")
        || visible.contains("GreenComplete")
        || visible.contains("Workflow complete")
        || visible.contains("DocsUpdated")
        || visible.contains("Type your feature")
        || visible.contains("Planning→Planned");

    assert!(
        progressed,
        "Keyboard inputs should advance the workflow; stripped text (len {}): {:?}",
        visible.len(),
        &visible[..visible.len().min(500)]
    );

    Ok(())
}

/// VirtualTui must keep rendering during clarification wait so highlights stay coherent,
/// but the **idle** status animation should follow ~1 Hz dot pulse + frozen elapsed — not
/// ~200ms full-frame churn driven by the fast spinner (PRD: tui-idle-status-loader).
///
/// Setup: connect gRPC terminal, reach Select mode (no further PresenterEvents), send no
/// input. We still expect autonomous output (periodic refresh), but **few** streamed chunks
/// in a 2s window when only the idle dot phase would change (~1 Hz), not one per 200ms tick.
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
    let preview: String = initial_text.chars().take(200).collect();
    eprintln!(
        "[TEST] initial burst: {} bytes, text preview: {:?}",
        initial.len(),
        preview
    );

    // Now: presenter is in Select mode, waiting for user input.
    // No PresenterEvents will arrive. No keyboard input is sent.
    // We still expect *some* autonomous output for responsive UI, but not ~10 full frames
    // in 2s from a 200ms periodic timer when the only visible animation is a 1 Hz idle dot.
    let chunk_count =
        count_terminal_chunks_in_window(&mut stream, Duration::from_secs(2), "idle-cadence")
            .await?;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx);

    assert!(
        chunk_count > 0,
        "VirtualTui should still emit occasional frames during Select wait (responsive UI), \
         but received 0 chunks in 2 seconds."
    );
    assert!(
        chunk_count <= 5,
        "PRD: idle clarification wait should not stream a full status-bar update every \
         ~200ms (~10 chunks / 2s); expect ~1 Hz idle animation cadence, got {chunk_count} chunks"
    );

    Ok(())
}

/// Acceptance (PRD): `virtual_tui_idle_animation_cadence` — during Select wait, streamed chunks
/// over a 2s window must stay in the ~1 Hz range (not fast-spinner / 200ms cadence).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn grpc_virtual_tui_idle_animation_cadence() -> anyhow::Result<()> {
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

    input_tx.send(TerminalInput { data: vec![] }).await?;

    let initial = drain_output(&mut stream, Duration::from_millis(500), "init-burst").await?;
    let initial_text = ansi_to_text(&initial);
    assert!(
        initial.len() > 50,
        "Should receive initial TUI render, got {} bytes",
        initial.len()
    );
    let preview_300: String = initial_text.chars().take(300).collect();
    assert!(
        initial_text.contains("Email/password") || initial_text.contains("Scope"),
        "Should reach Select mode; got: {:?}",
        preview_300
    );

    let chunks =
        count_terminal_chunks_in_window(&mut stream, Duration::from_secs(2), "cadence").await?;

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx);

    assert!(
        chunks <= 5,
        "PRD: VirtualTui idle animation should not emit ~200ms spinner-driven frames in \
         Select wait; expect at most ~5 chunks / 2s (~1 Hz dot), got {chunks}"
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
    let preview_300: String = initial_text.chars().take(300).collect();
    assert!(
        initial_text.contains("Email/password") || initial_text.contains("Scope"),
        "Should reach Select mode with authentication question; got: {:?}",
        preview_300
    );

    // Feed initial output into vt100 parser to verify initial state:
    // first option "Email/password" should have "> " prefix (selected).
    let mut parser = ScreenParser::new(24, 80);
    parser.feed(&initial);
    let before_screen = parser.contents();
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
    let mut chunk_parser = ScreenParser::new(24, 80);
    chunk_parser.feed(&initial); // start from same state
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut chunk_idx = 0u32;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(300), stream.message()).await {
            Ok(Ok(Some(output))) => {
                chunk_idx += 1;
                chunk_parser.feed(&output.data);
                let screen = chunk_parser.contents();
                if screen.contains("> Email/password") && !screen.contains("> OAuth") {
                    chunks_with_selection_reset.push((chunk_idx, screen.clone()));
                }
                parser.feed(&output.data);
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(anyhow::anyhow!("stream error: {}", e)),
            Err(_) => break,
        }
    }
    let after_screen = parser.contents();
    eprintln!(
        "[TEST] after Down + periodic renders — screen:\n{}",
        after_screen
    );
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

/// Bug reproduction: when shrinking and growing the terminal by 10 rows, the final
/// visible screen should show "PgUp/PgDn scroll" exactly once. Resize artifacts
/// (duplicate status bars, scrollback accumulation, or layout glitches) can cause
/// it to appear multiple times in the visible output.
#[tokio::test]
async fn grpc_resize_shrink_grow_shows_pgup_pgdn_scroll_exactly_once() -> anyhow::Result<()> {
    let (_handle, port, shutdown) =
        spawn_presenter_with_terminal_service(Some("Build auth".to_string()));

    let mut client = connect_terminal_grpc(port).await?;

    let (input_tx, input_rx) = tokio::sync::mpsc::channel(64);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let mut stream = client
        .stream_terminal_io(tonic::Request::new(input_stream))
        .await?
        .into_inner();

    input_tx
        .send(TerminalInput {
            data: encode_resize(80, 24),
        })
        .await?;

    let mut all_output = drain_output(&mut stream, Duration::from_millis(500), "init").await?;

    input_tx
        .send(TerminalInput {
            data: encode_resize(80, 14),
        })
        .await?;
    let shrink_output = drain_output(&mut stream, Duration::from_millis(500), "shrink").await?;
    all_output.extend_from_slice(&shrink_output);

    input_tx
        .send(TerminalInput {
            data: encode_resize(80, 24),
        })
        .await?;
    let grow_output = drain_output(&mut stream, Duration::from_millis(500), "grow").await?;
    all_output.extend_from_slice(&grow_output);

    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(input_tx);

    let mut parser = ScreenParser::new(24, 80);
    parser.feed(&all_output);
    let visible = parser.contents();
    let count = visible.matches("PgUp/PgDn scroll").count();
    assert_eq!(
        count, 1,
        "PgUp/PgDn scroll should appear exactly once in final visible screen after shrink and grow by 10 rows; got {} occurrences. Screen:\n{}",
        count,
        visible
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::await_holding_lock)]
async fn grpc_virtual_tui_rpc_large_echo_char_by_char() -> anyhow::Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    std::env::set_var("TDDY_E2E_NO_ENTER_AFFORDANCE", "1");

    const COLS: u16 = 80;
    const ROWS: u16 = 10000;
    let max_height = (ROWS as usize / 3).max(1);
    let max_feature_len = max_height.saturating_mul(COLS as usize).saturating_sub(2);
    let feature_len = max_feature_len.min(LARGE_ECHO_CHAR_CAP);
    let (expected, segments) = build_large_echo_segmented_payload(feature_len, LARGE_ECHO_SEGMENTS);

    let all_output = {
        let _lock = LARGE_ECHO_TEST_LOCK
            .lock()
            .expect("large echo test serialization");

        let (_handle, port, shutdown) = spawn_presenter_with_terminal_service(None);

        let mut client = connect_terminal_grpc(port).await?;

        let (input_tx, input_rx) = tokio::sync::mpsc::channel::<TerminalInput>(1024);
        let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

        let mut stream = client
            .stream_terminal_io(tonic::Request::new(input_stream))
            .await?
            .into_inner();

        let buf = Arc::new(Mutex::new(Vec::new()));
        let buf_for_reader = Arc::clone(&buf);
        let reader = tokio::spawn(async move {
            loop {
                match stream.message().await {
                    Ok(Some(o)) => buf_for_reader
                        .lock()
                        .expect("terminal output buffer")
                        .extend_from_slice(&o.data),
                    Ok(None) => break,
                    Err(e) => return Err(anyhow::anyhow!("terminal stream: {}", e)),
                }
            }
            Ok::<(), anyhow::Error>(())
        });

        input_tx
            .send(TerminalInput {
                data: encode_resize(COLS, ROWS),
            })
            .await?;
        input_tx.send(TerminalInput { data: vec![] }).await?;
        for byte in expected.as_bytes() {
            input_tx.send(TerminalInput { data: vec![*byte] }).await?;
        }

        eventually_segmented_echo(
            &buf,
            expected.as_str(),
            ROWS,
            COLS,
            SegmentedEchoWaitParams {
                timeout: LARGE_ECHO_VT100_SYNC_TIMEOUT,
                min_interval: LARGE_ECHO_VT100_SYNC_MIN_INTERVAL,
                min_new_bytes: LARGE_ECHO_VT100_SYNC_MIN_NEW_BYTES,
                loop_sleep: LARGE_ECHO_VT100_SYNC_LOOP_SLEEP,
                style: SegmentedEchoFailureStyle::Grpc,
            },
        )
        .await;

        drop(input_tx);
        shutdown.store(true, Ordering::Relaxed);

        reader.await??;

        let all_output = buf.lock().expect("terminal output buffer").clone();
        all_output
    };

    assert_segmented_echo(
        &all_output,
        &expected,
        &segments,
        ROWS,
        COLS,
        SegmentedEchoFailureStyle::Grpc,
    );

    Ok(())
}

/// Same long echo and vt100 contiguous-prefix check as
/// [`grpc_virtual_tui_rpc_large_echo_char_by_char`], but drives
/// [`tddy_service::start_virtual_tui_session`] input/output channels directly (no tonic RPC).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::await_holding_lock)]
async fn virtual_tui_large_echo_char_by_char_direct_vt100() -> anyhow::Result<()> {
    std::env::set_var("TDDY_E2E_NO_ENTER_AFFORDANCE", "1");

    const COLS: u16 = 80;
    const ROWS: u16 = 10000;
    let max_height = (ROWS as usize / 3).max(1);
    let max_feature_len = max_height.saturating_mul(COLS as usize).saturating_sub(2);
    let feature_len = max_feature_len.min(LARGE_ECHO_CHAR_CAP);
    let (expected, segments) = build_large_echo_segmented_payload(feature_len, LARGE_ECHO_SEGMENTS);

    let all_output = {
        let _lock = LARGE_ECHO_TEST_LOCK
            .lock()
            .expect("large echo test serialization");

        let (_presenter_handle, factory, presenter_shutdown) =
            spawn_presenter_with_view_factory(None);

        let Some(session) = start_virtual_tui_session(&*factory, false) else {
            anyhow::bail!("connect_view / start_virtual_tui_session");
        };
        let VirtualTuiSession {
            input_tx,
            output_rx,
            shutdown: vt_shutdown,
        } = session;

        let buf = Arc::new(Mutex::new(Vec::new()));
        let buf_for_reader = Arc::clone(&buf);
        let reader = tokio::spawn(async move {
            let mut rx = output_rx;
            while let Some(chunk) = rx.recv().await {
                buf_for_reader
                    .lock()
                    .expect("virtual tui output buffer")
                    .extend_from_slice(&chunk);
            }
        });

        input_tx
            .send(encode_resize(COLS, ROWS))
            .await
            .map_err(|e| anyhow::anyhow!("input_tx resize: {}", e))?;
        input_tx
            .send(vec![])
            .await
            .map_err(|e| anyhow::anyhow!("input_tx empty: {}", e))?;
        for byte in expected.as_bytes() {
            input_tx
                .send(vec![*byte])
                .await
                .map_err(|e| anyhow::anyhow!("input_tx byte: {}", e))?;
        }

        eventually_segmented_echo(
            &buf,
            expected.as_str(),
            ROWS,
            COLS,
            SegmentedEchoWaitParams {
                timeout: LARGE_ECHO_VT100_SYNC_TIMEOUT,
                min_interval: LARGE_ECHO_VT100_SYNC_MIN_INTERVAL,
                min_new_bytes: LARGE_ECHO_VT100_SYNC_MIN_NEW_BYTES,
                loop_sleep: LARGE_ECHO_VT100_SYNC_LOOP_SLEEP,
                style: SegmentedEchoFailureStyle::Grpc,
            },
        )
        .await;

        drop(input_tx);
        vt_shutdown.store(true, Ordering::Relaxed);
        presenter_shutdown.store(true, Ordering::Relaxed);

        reader.await?;

        let all_output = buf.lock().expect("virtual tui output buffer").clone();
        all_output
    };

    assert_segmented_echo(
        &all_output,
        &expected,
        &segments,
        ROWS,
        COLS,
        SegmentedEchoFailureStyle::Grpc,
    );

    Ok(())
}
