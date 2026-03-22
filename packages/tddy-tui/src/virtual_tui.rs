//! VirtualTui: headless ratatui renderer for per-connection terminal streaming.
//!
//! Subscribes to PresenterEvent, maintains local state, renders via CrosstermBackend
//! to a headless CapturingWriter, and streams ANSI bytes to the connected client.
//! Processes client input bytes into UserIntents.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui::{TerminalOptions, Viewport};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::mpsc;

use tddy_core::{
    AppMode, PresenterEvent, PresenterState, PresenterView, UserIntent, ViewConnection,
};

use crate::capturing_writer::CapturingWriter;
use crate::ctrl_interrupt::{ctrl_c_interrupt_session, key_is_ctrl_c_press};
use crate::key_map::key_event_to_intent;
use crate::mouse_map::{handle_mouse_event, LayoutAreas};
use crate::render::draw;
use crate::tui_view::TuiView;

/// Runs a VirtualTui in a dedicated thread. Renders on events, streams ANSI bytes.
/// Stops when shutdown is set or output_tx is dropped.
pub fn run_virtual_tui(
    conn: ViewConnection,
    output_tx: mpsc::Sender<Vec<u8>>,
    input_rx: mpsc::Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
    mouse: bool,
) {
    thread::spawn(move || {
        let mut state = conn.state_snapshot;
        let mut view = TuiView::new();
        let mut input_buf: Vec<u8> = Vec::new();

        // Collect each draw()'s raw ANSI output into a buffer so we can diff
        // against the previous frame and only send bytes when content changed.
        let frame_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let on_write = {
            let buf = frame_buf.clone();
            move |bytes: &[u8]| {
                if let Ok(mut b) = buf.lock() {
                    b.extend_from_slice(bytes);
                }
            }
        };
        let writer = CapturingWriter::headless(Box::new(on_write));
        let backend = CrosstermBackend::new(writer);
        // Use fixed viewport to avoid crossterm::terminal::size() which fails without a TTY (daemon/headless).
        let viewport = Viewport::Fixed(Rect::new(0, 0, 80, 24));
        let mut terminal = match Terminal::with_options(backend, TerminalOptions { viewport }) {
            Ok(t) => t,
            Err(e) => {
                log::error!("VirtualTui: failed to create terminal: {}", e);
                return;
            }
        };

        let mut prev_frame: Vec<u8> = Vec::new();

        log::debug!("VirtualTui: started (mouse={})", mouse);

        // Render a frame: draw into the buffer, compare with the previous frame,
        // and only send bytes to the output channel if content actually changed.
        // This avoids sending cursor-only control sequences from ratatui's draw()
        // on identical frames, which would flood the network stream.
        let mut layout_areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        let render_and_send = |term: &mut Terminal<CrosstermBackend<CapturingWriter>>,
                               state: &PresenterState,
                               view: &mut TuiView,
                               frame_buf: &Arc<Mutex<Vec<u8>>>,
                               prev_frame: &mut Vec<u8>,
                               output_tx: &mpsc::Sender<Vec<u8>>,
                               layout_areas: &mut LayoutAreas| {
            {
                let mut b = frame_buf.lock().unwrap();
                b.clear();
            }
            if let Err(e) =
                term.draw(|f| draw(f, state, view.view_state_mut(), false, Some(layout_areas)))
            {
                log::debug!("VirtualTui: draw error: {}", e);
                return;
            }
            let current_frame = {
                let b = frame_buf.lock().unwrap();
                b.clone()
            };
            if current_frame != *prev_frame {
                log::debug!(
                    "VirtualTui: frame changed {} bytes -> client",
                    current_frame.len()
                );
                // When prev_frame is empty (initial render or post-resize), prepend clear
                // so the remote vt100 parser starts with a clean slate. Otherwise shrink→grow
                // leaves old content visible and the final screen shows duplicated status bars.
                let to_send: Vec<u8> = if prev_frame.is_empty() {
                    const CLEAR_AND_HOME: &[u8] = b"\x1b[2J\x1b[H";
                    let mut out = Vec::with_capacity(CLEAR_AND_HOME.len() + current_frame.len());
                    out.extend_from_slice(CLEAR_AND_HOME);
                    out.extend_from_slice(&current_frame);
                    out
                } else {
                    current_frame.clone()
                };
                let _ = output_tx.blocking_send(to_send);
                *prev_frame = current_frame;
            }
        };

        render_and_send(
            &mut terminal,
            &state,
            &mut view,
            &frame_buf,
            &mut prev_frame,
            &output_tx,
            &mut layout_areas,
        );

        if mouse {
            {
                let mut b = frame_buf.lock().unwrap();
                b.clear();
            }
            if execute!(terminal.backend_mut(), crossterm::event::EnableMouseCapture).is_ok() {
                let seq = {
                    let b = frame_buf.lock().unwrap();
                    b.clone()
                };
                if !seq.is_empty() {
                    let _ = output_tx.blocking_send(seq);
                }
            }
        }

        let mut input_rx = input_rx;
        let mut event_rx = conn.event_rx;
        let intent_tx = conn.intent_tx;

        // Periodic render interval to keep animations alive (spinner, elapsed timer),
        // matching the real TUI event loop (event_loop.rs:69) which draws every ~50ms.
        let render_interval = Duration::from_millis(200);
        let mut last_render = std::time::Instant::now();

        let mut recv_chunk_count: u64 = 0;
        let mut total_input_bytes: u64 = 0;
        let mut total_keys_parsed: u64 = 0;

        loop {
            let mut updated = false;

            let had_events = drain_presenter_broadcast(&mut event_rx, &mut state, &mut view);
            if had_events {
                log::debug!("VirtualTui: drained PresenterEvents from broadcast");
                updated = true;
            }

            loop {
                match input_rx.try_recv() {
                    Ok(bytes) if !bytes.is_empty() => {
                        process_virtual_tui_input_chunk(
                            &bytes,
                            &mut updated,
                            &mut recv_chunk_count,
                            &mut total_input_bytes,
                            &mut total_keys_parsed,
                            &mut input_buf,
                            &mut terminal,
                            &mut prev_frame,
                            mouse,
                            &state,
                            &mut view,
                            &layout_areas,
                            &intent_tx,
                            &shutdown,
                        );
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        log::debug!(
                            "VirtualTui: input_rx disconnected recv_chunks={} total_input_bytes={} total_keys_parsed={}",
                            recv_chunk_count,
                            total_input_bytes,
                            total_keys_parsed
                        );
                        break;
                    }
                    _ => break,
                }
            }

            // Render on events/input immediately, or periodically to keep the
            // spinner and elapsed timer alive.
            let render_reason = if updated {
                "events/input"
            } else if last_render.elapsed() >= render_interval {
                "periodic"
            } else {
                ""
            };
            if !render_reason.is_empty() {
                log::debug!("VirtualTui: render ({})", render_reason);
                render_and_send(
                    &mut terminal,
                    &state,
                    &mut view,
                    &frame_buf,
                    &mut prev_frame,
                    &output_tx,
                    &mut layout_areas,
                );
                last_render = std::time::Instant::now();
            }

            thread::sleep(Duration::from_millis(10));

            if shutdown.load(Ordering::Relaxed) {
                let mut straggler_updated = false;
                loop {
                    match input_rx.try_recv() {
                        Ok(bytes) if !bytes.is_empty() => {
                            process_virtual_tui_input_chunk(
                                &bytes,
                                &mut straggler_updated,
                                &mut recv_chunk_count,
                                &mut total_input_bytes,
                                &mut total_keys_parsed,
                                &mut input_buf,
                                &mut terminal,
                                &mut prev_frame,
                                mouse,
                                &state,
                                &mut view,
                                &layout_areas,
                                &intent_tx,
                                &shutdown,
                            );
                        }
                        Ok(_) => {}
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                    }
                }
                if straggler_updated {
                    log::debug!("VirtualTui: render (stragglers after shutdown)");
                    render_and_send(
                        &mut terminal,
                        &state,
                        &mut view,
                        &frame_buf,
                        &mut prev_frame,
                        &output_tx,
                        &mut layout_areas,
                    );
                }
                log::debug!(
                    "VirtualTui: shutdown set after drain/render; exiting recv_chunks={} total_input_bytes={} total_keys_parsed={}",
                    recv_chunk_count,
                    total_input_bytes,
                    total_keys_parsed
                );
                break;
            }
        }
        log::debug!("VirtualTui: main loop exited");
    });
}

#[allow(clippy::too_many_arguments)]
fn process_virtual_tui_input_chunk(
    bytes: &[u8],
    updated: &mut bool,
    recv_chunk_count: &mut u64,
    total_input_bytes: &mut u64,
    total_keys_parsed: &mut u64,
    input_buf: &mut Vec<u8>,
    terminal: &mut Terminal<CrosstermBackend<CapturingWriter>>,
    prev_frame: &mut Vec<u8>,
    mouse: bool,
    state: &PresenterState,
    view: &mut TuiView,
    layout_areas: &LayoutAreas,
    intent_tx: &std_mpsc::Sender<UserIntent>,
    shutdown: &Arc<AtomicBool>,
) {
    *recv_chunk_count += 1;
    let chunk_len = bytes.len() as u64;
    *total_input_bytes += chunk_len;
    if *recv_chunk_count <= 5 || (*recv_chunk_count).is_multiple_of(500) || chunk_len != 1 {
        log::debug!(
            "VirtualTui: recv chunk#{} len={} total_input_bytes={} buf_after_extend_will_be={}",
            *recv_chunk_count,
            chunk_len,
            *total_input_bytes,
            input_buf.len() + bytes.len()
        );
    }
    input_buf.extend_from_slice(bytes);
    while let Some((cols, rows, consumed)) = parse_resize_from_buf(input_buf) {
        log::debug!("VirtualTui: resize {}x{}", cols, rows);
        apply_resize(terminal, prev_frame, cols, rows);
        input_buf.drain(..consumed);
        *updated = true;
    }
    if mouse {
        while let Some((mouse_ev, consumed)) = parse_mouse_from_buf(input_buf) {
            log::debug!("VirtualTui: mouse {:?}", mouse_ev.kind);
            let normalized = crate::mouse_map::normalize_mouse_coords_for_local(mouse_ev);
            if let Some(intent) = handle_mouse_event(
                normalized,
                &state.mode,
                view.view_state_mut(),
                layout_areas,
                state.inbox.len(),
            ) {
                let _ = intent_tx.send(intent);
            }
            input_buf.drain(..consumed);
            *updated = true;
        }
    }
    while let Some((key, consumed)) = parse_key_from_buf(input_buf) {
        *total_keys_parsed += 1;
        log::trace!(
            "VirtualTui: key {:?} mode={:?} total_keys_parsed={}",
            key.code,
            state.mode,
            *total_keys_parsed
        );
        if key_is_ctrl_c_press(&key) {
            ctrl_c_interrupt_session(shutdown.as_ref());
            input_buf.drain(..consumed);
            *updated = true;
            continue;
        }
        let inbox_len = state.inbox.len();
        let view_consumed =
            view.view_state_mut()
                .handle_key_view_local(key, &state.mode, inbox_len);
        if view_consumed {
            log::trace!("VirtualTui: key {:?} consumed by view", key.code);
            if matches!(state.mode, AppMode::FeatureInput) {
                let flen = view.view_state().feature_input.len();
                if flen.is_multiple_of(250) || flen <= 8 {
                    log::debug!(
                        "VirtualTui: FeatureInput progress feature_input_len={} total_input_bytes={} total_keys_parsed={}",
                        flen,
                        *total_input_bytes,
                        *total_keys_parsed
                    );
                }
            }
            *updated = true;
        } else if let Some(intent) = key_event_to_intent(key, &state.mode, view.view_state()) {
            log::debug!("VirtualTui: intent {:?} -> presenter", intent);
            let _ = intent_tx.send(intent);
            *updated = true;
        }
        input_buf.drain(..consumed);
    }
    if !input_buf.is_empty() {
        log::debug!(
            "VirtualTui: after key parse, input_buf still has {} bytes (partial escape/utf8?)",
            input_buf.len()
        );
    }
}

/// Apply resize: resize terminal, clear buffers, reset prev_frame.
/// Ensures the next render sends a full frame to the remote client.
fn apply_resize<B: Backend>(
    terminal: &mut Terminal<B>,
    prev_frame: &mut Vec<u8>,
    cols: u16,
    rows: u16,
) {
    if let Err(e) = terminal.resize(Rect::new(0, 0, cols, rows)) {
        log::debug!("virtual_tui: resize error: {}", e);
    }
    if let Err(e) = terminal.clear() {
        log::debug!("virtual_tui: clear after resize error: {}", e);
    }
    prev_frame.clear();
}

/// Drain all pending [`PresenterEvent`]s from a broadcast subscription.
///
/// [`broadcast::Receiver::try_recv`] may return [`TryRecvError::Lagged`] without yielding a
/// value; the cursor then points at the oldest retained message, which the next `try_recv`
/// returns. A plain `while let Ok(ev) = try_recv()` stops on `Lagged` and defers processing
/// until a later poll. This loop retries on `Lagged` (same idea as tokio's `Receiver` drop
/// implementation) without spinning on empty.
pub(crate) fn drain_presenter_broadcast(
    event_rx: &mut tokio::sync::broadcast::Receiver<PresenterEvent>,
    state: &mut PresenterState,
    view: &mut TuiView,
) -> bool {
    let mut any = false;
    loop {
        match event_rx.try_recv() {
            Ok(ev) => {
                log::debug!(
                    "VirtualTui: PresenterEvent {:?}",
                    std::mem::discriminant(&ev)
                );
                apply_event(state, view, ev);
                any = true;
            }
            Err(TryRecvError::Lagged(_)) => continue,
            Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
        }
    }
    any
}

pub fn apply_event(state: &mut PresenterState, view: &mut TuiView, ev: PresenterEvent) {
    use std::time::Instant;

    match ev {
        PresenterEvent::ModeChanged(mode) => {
            state.mode = mode.clone();
            view.on_mode_changed(&mode);
        }
        PresenterEvent::ActivityLogged(entry) => {
            state.activity_log.push(entry.clone());
            view.on_activity_logged(&entry, state.activity_log.len());
        }
        PresenterEvent::GoalStarted(goal) => {
            state.current_goal = Some(goal.clone());
            state.goal_start_time = Instant::now();
            if matches!(state.mode, AppMode::FeatureInput) {
                state.mode = AppMode::Running;
                view.on_mode_changed(&state.mode);
            }
            view.on_goal_started(&goal);
        }
        PresenterEvent::StateChanged { from, to } => {
            state.current_state = Some(to.clone());
            view.on_state_changed(&from, &to);
        }
        PresenterEvent::InboxChanged(inbox) => {
            state.inbox = inbox;
            view.on_inbox_changed(&state.inbox);
        }
        PresenterEvent::WorkflowComplete(ref result) => {
            state.mode = match result {
                Ok(_) => AppMode::FeatureInput,
                Err(_) => AppMode::ErrorRecovery {
                    error_message: result.as_ref().err().cloned().unwrap_or_default(),
                },
            };
            view.on_workflow_complete(result);
        }
        PresenterEvent::AgentOutput(text) => {
            view.on_agent_output(&text);
        }
        PresenterEvent::IntentReceived(UserIntent::Quit) => {
            state.should_quit = true;
        }
        PresenterEvent::IntentReceived(_) => {}
        PresenterEvent::BackendSelected { .. } => {}
        PresenterEvent::SessionRuntimeStatus(_) => {
            // Remote-only snapshot; local TUI renders status from state + periodic redraw.
        }
        PresenterEvent::ShouldQuit => {
            state.should_quit = true;
        }
    }
}

/// Parse resize escape sequence from buffer. Format: \x1b]resize;{cols};{rows}\x07
/// Returns (cols, rows, bytes_consumed) or None if incomplete/not found.
fn parse_resize_from_buf(buf: &[u8]) -> Option<(u16, u16, usize)> {
    let prefix = b"\x1b]resize;";
    if buf.len() < prefix.len() || !buf.starts_with(prefix) {
        return None;
    }
    let rest = &buf[prefix.len()..];
    let semicolon = rest.iter().position(|&b| b == b';')?;
    let cols_str = std::str::from_utf8(&rest[..semicolon]).ok()?;
    let cols: u16 = cols_str.parse().ok()?;
    let after_semicolon = &rest[semicolon + 1..];
    let bel = after_semicolon.iter().position(|&b| b == 0x07)?;
    let rows_str = std::str::from_utf8(&after_semicolon[..bel]).ok()?;
    let rows: u16 = rows_str.parse().ok()?;
    let consumed = prefix.len() + semicolon + 1 + bel + 1;
    Some((cols, rows, consumed))
}

/// Parse SGR mouse sequence from buffer. Format: ESC [ < Pb ; Px ; Py M or m
/// Returns (MouseEvent, bytes_consumed) or None if incomplete/not found.
fn parse_mouse_from_buf(buf: &[u8]) -> Option<(crossterm::event::MouseEvent, usize)> {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let prefix = b"\x1b[<";
    if buf.len() < prefix.len() + 5 || !buf.starts_with(prefix) {
        return None;
    }
    let mut pos = prefix.len();
    let mut rest = &buf[pos..];

    let mut i = 0;
    while i < rest.len() && rest[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= rest.len() || rest[i] != b';' {
        return None;
    }
    let pb: u8 = std::str::from_utf8(&rest[..i]).ok()?.parse().ok()?;
    pos += i + 1;
    rest = &buf[pos..];
    i = 0;
    while i < rest.len() && rest[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= rest.len() || rest[i] != b';' {
        return None;
    }
    let px: u16 = std::str::from_utf8(&rest[..i]).ok()?.parse().ok()?;
    pos += i + 1;
    rest = &buf[pos..];
    i = 0;
    while i < rest.len() && (rest[i].is_ascii_digit() || rest[i] == b' ') {
        i += 1;
    }
    if i == 0 || i >= rest.len() {
        return None;
    }
    let py: u16 = std::str::from_utf8(&rest[..i]).ok()?.trim().parse().ok()?;
    let last = rest[i];
    let kind = match (pb, last) {
        (0, b'M') => MouseEventKind::Down(crossterm::event::MouseButton::Left),
        (0, b'm') => MouseEventKind::Up(crossterm::event::MouseButton::Left),
        (64, b'M') => MouseEventKind::ScrollUp,
        (65, b'M') => MouseEventKind::ScrollDown,
        _ => return None,
    };
    let consumed = pos + i + 1;
    let event = MouseEvent {
        kind,
        column: px.saturating_sub(1),
        row: py.saturating_sub(1),
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    Some((event, consumed))
}

/// Parse one key event from the buffer. Returns (KeyEvent, bytes_consumed) or None if incomplete.
fn parse_key_from_buf(buf: &mut [u8]) -> Option<(KeyEvent, usize)> {
    if buf.is_empty() {
        return None;
    }
    if buf[0] == b'\r' || buf[0] == b'\n' {
        return Some((
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::empty(), KeyEventKind::Press),
            1,
        ));
    }
    if buf[0] == 0x1b {
        if buf.len() >= 2 {
            if buf[1] == b'[' {
                if buf.len() >= 3 {
                    match buf[2] {
                        b'A' => {
                            return Some((
                                KeyEvent::new_with_kind(
                                    KeyCode::Up,
                                    KeyModifiers::empty(),
                                    KeyEventKind::Press,
                                ),
                                3,
                            ))
                        }
                        b'B' => {
                            return Some((
                                KeyEvent::new_with_kind(
                                    KeyCode::Down,
                                    KeyModifiers::empty(),
                                    KeyEventKind::Press,
                                ),
                                3,
                            ))
                        }
                        b'5' if buf.len() >= 4 && buf[3] == b'~' => {
                            return Some((
                                KeyEvent::new_with_kind(
                                    KeyCode::PageUp,
                                    KeyModifiers::empty(),
                                    KeyEventKind::Press,
                                ),
                                4,
                            ))
                        }
                        b'6' if buf.len() >= 4 && buf[3] == b'~' => {
                            return Some((
                                KeyEvent::new_with_kind(
                                    KeyCode::PageDown,
                                    KeyModifiers::empty(),
                                    KeyEventKind::Press,
                                ),
                                4,
                            ))
                        }
                        _ => {}
                    }
                }
            } else if buf[1] == b'O' && buf.len() >= 3 {
                match buf[2] {
                    b'A' => {
                        return Some((
                            KeyEvent::new_with_kind(
                                KeyCode::Up,
                                KeyModifiers::empty(),
                                KeyEventKind::Press,
                            ),
                            3,
                        ))
                    }
                    b'B' => {
                        return Some((
                            KeyEvent::new_with_kind(
                                KeyCode::Down,
                                KeyModifiers::empty(),
                                KeyEventKind::Press,
                            ),
                            3,
                        ))
                    }
                    _ => {}
                }
            }
        }
        return None;
    }
    if buf[0] == b'q' || buf[0] == b'Q' {
        return Some((
            KeyEvent::new_with_kind(
                KeyCode::Char(buf[0] as char),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ),
            1,
        ));
    }
    if buf[0] == 3 {
        return Some((
            KeyEvent::new_with_kind(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            1,
        ));
    }
    if buf[0] == 0x7f {
        return Some((
            KeyEvent::new_with_kind(
                KeyCode::Backspace,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ),
            1,
        ));
    }
    if buf[0] == b'\t' {
        return Some((
            KeyEvent::new_with_kind(KeyCode::Tab, KeyModifiers::empty(), KeyEventKind::Press),
            1,
        ));
    }
    if buf[0].is_ascii() && !buf[0].is_ascii_control() {
        return Some((
            KeyEvent::new_with_kind(
                KeyCode::Char(buf[0] as char),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ),
            1,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_enter() {
        let mut buf = vec![b'\r'];
        let (key, n) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(key.code, KeyCode::Enter);
    }

    #[test]
    fn parse_page_up() {
        let mut buf = vec![0x1b, b'[', b'5', b'~'];
        let (key, n) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(key.code, KeyCode::PageUp);
    }

    #[test]
    fn parse_page_down() {
        let mut buf = vec![0x1b, b'[', b'6', b'~'];
        let (key, n) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(key.code, KeyCode::PageDown);
    }

    #[test]
    fn parse_backspace() {
        let mut buf = vec![0x7f];
        let (key, n) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(key.code, KeyCode::Backspace);
    }

    #[test]
    fn keys_after_backspace_are_still_parsed() {
        let mut buf = vec![0x7f, b'a'];

        let (key1, consumed1) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(key1.code, KeyCode::Backspace);
        buf.drain(..consumed1);

        let (key2, _) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(key2.code, KeyCode::Char('a'));
    }

    #[test]
    fn parse_tab() {
        let mut buf = vec![b'\t'];
        let (key, n) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(key.code, KeyCode::Tab);
    }

    #[test]
    fn parse_resize_sequence() {
        // \x1b]resize;120;30\x07
        let buf = vec![
            0x1b, b']', b'r', b'e', b's', b'i', b'z', b'e', b';', b'1', b'2', b'0', b';', b'3',
            b'0', 0x07,
        ];
        let (cols, rows, consumed) = parse_resize_from_buf(&buf).unwrap();
        assert_eq!(cols, 120);
        assert_eq!(rows, 30);
        assert_eq!(consumed, 16);
    }

    #[test]
    fn parse_sgr_mouse_press() {
        // ESC [ < 0 ; 10 ; 5 M (left click at col 10, row 5)
        let buf = vec![
            0x1b, b'[', b'<', b'0', b';', b'1', b'0', b';', b'5', b' ', b'M',
        ];
        let (event, consumed) = parse_mouse_from_buf(&buf).unwrap();
        assert_eq!(consumed, 11);
        assert_eq!(event.row, 4); // 0-based
        assert_eq!(event.column, 9); // 0-based
        assert!(matches!(
            event.kind,
            crossterm::event::MouseEventKind::Down(_)
        ));
    }

    #[test]
    fn parse_sgr_mouse_scroll_down() {
        // ESC [ < 65 ; 1 ; 1 M (scroll down)
        let buf = vec![
            0x1b, b'[', b'<', b'6', b'5', b';', b'1', b';', b'1', b' ', b'M',
        ];
        let (event, consumed) = parse_mouse_from_buf(&buf).unwrap();
        assert_eq!(consumed, 11);
        assert!(matches!(
            event.kind,
            crossterm::event::MouseEventKind::ScrollDown
        ));
    }

    #[test]
    fn apply_resize_clears_prev_frame() {
        use ratatui::backend::TestBackend;
        use ratatui::{TerminalOptions, Viewport};

        let backend = TestBackend::new(80, 24);
        let viewport = Viewport::Fixed(Rect::new(0, 0, 80, 24));
        let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();

        let mut prev_frame = vec![1u8, 2, 3];
        apply_resize(&mut terminal, &mut prev_frame, 60, 12);

        assert!(
            prev_frame.is_empty(),
            "apply_resize must clear prev_frame so next render sends full frame"
        );
    }

    #[test]
    fn resize_and_clear_then_draw_produces_correct_frame_area() {
        use ratatui::backend::TestBackend;
        use ratatui::widgets::Paragraph;
        use ratatui::{TerminalOptions, Viewport};

        // Use Fixed viewport (like virtual_tui) so resize() updates dimensions.
        // Verifies resize+clear+draw contract: frame area matches resized dimensions.
        let backend = TestBackend::new(80, 24);
        let viewport = Viewport::Fixed(Rect::new(0, 0, 80, 24));
        let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();

        terminal.resize(Rect::new(0, 0, 60, 12)).unwrap();
        terminal.clear().unwrap();

        let mut frame_area = Rect::default();
        terminal
            .draw(|f| {
                frame_area = f.area();
                f.render_widget(Paragraph::new("x"), frame_area);
            })
            .unwrap();

        assert_eq!(frame_area.width, 60, "frame width should match resize");
        assert_eq!(frame_area.height, 12, "frame height should match resize");
    }

    #[test]
    fn keys_after_mouse_release_are_still_parsed() {
        let mut buf = vec![
            0x1b, b'[', b'<', b'0', b';', b'1', b'0', b';', b'5', b' ', b'M', 0x1b, b'[', b'<',
            b'0', b';', b'1', b'0', b';', b'5', b' ', b'm', b'a',
        ];

        let (mouse1, consumed1) = parse_mouse_from_buf(&buf).unwrap();
        assert!(matches!(
            mouse1.kind,
            crossterm::event::MouseEventKind::Down(_)
        ));
        buf.drain(..consumed1);

        let (mouse2, consumed2) = parse_mouse_from_buf(&buf).unwrap();
        assert!(matches!(
            mouse2.kind,
            crossterm::event::MouseEventKind::Up(_)
        ));
        buf.drain(..consumed2);

        let (key, _) = parse_key_from_buf(&mut buf).unwrap();
        assert_eq!(key.code, KeyCode::Char('a'));
    }

    /// Capacity 2 + three sends without reading forces `TryRecvError::Lagged` on the first
    /// `try_recv`; the drain loop must retry so `IntentReceived(Quit)` still applies.
    #[test]
    fn drain_broadcast_retries_after_lagged_so_quit_applies() {
        use std::time::Instant;

        use tddy_core::presenter::{ActivityEntry, ActivityKind};
        use tddy_core::UserIntent;

        let (tx, mut rx) = tokio::sync::broadcast::channel(2);
        let entry = |text: &str| {
            PresenterEvent::ActivityLogged(ActivityEntry {
                text: text.to_string(),
                kind: ActivityKind::Info,
            })
        };
        tx.send(entry("a")).unwrap();
        tx.send(entry("b")).unwrap();
        tx.send(PresenterEvent::IntentReceived(UserIntent::Quit))
            .unwrap();

        let mut state = PresenterState {
            agent: String::new(),
            model: String::new(),
            mode: AppMode::FeatureInput,
            current_goal: None,
            current_state: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
        };
        let mut view = TuiView::new();

        assert!(
            drain_presenter_broadcast(&mut rx, &mut state, &mut view),
            "expected at least one event after lag"
        );
        assert!(
            state.should_quit,
            "Quit must apply in the same drain after Lagged"
        );
    }
}
