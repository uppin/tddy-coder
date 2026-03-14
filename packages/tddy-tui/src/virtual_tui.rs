//! VirtualTui: headless ratatui renderer for per-connection terminal streaming.
//!
//! Subscribes to PresenterEvent, maintains local state, renders via CrosstermBackend
//! to a headless CapturingWriter, and streams ANSI bytes to the connected client.
//! Processes client input bytes into UserIntents.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui::{TerminalOptions, Viewport};
use tokio::sync::mpsc;

use tddy_core::{AppMode, PresenterEvent, PresenterState, PresenterView, UserIntent, ViewConnection};

use crate::capturing_writer::CapturingWriter;
use crate::key_map::key_event_to_intent;
use crate::render::draw;
use crate::tui_view::TuiView;

/// Runs a VirtualTui in a dedicated thread. Renders on events, streams ANSI bytes.
/// Stops when shutdown is set or output_tx is dropped.
pub fn run_virtual_tui(
    conn: ViewConnection,
    output_tx: mpsc::Sender<Vec<u8>>,
    input_rx: mpsc::Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut state = conn.state_snapshot;
        let mut view = TuiView::new();
        let mut input_buf: Vec<u8> = Vec::new();

        let on_write = {
            let tx = output_tx.clone();
            move |buf: &[u8]| {
                let _ = tx.blocking_send(buf.to_vec());
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

        let render = |term: &mut Terminal<CrosstermBackend<CapturingWriter>>,
                      state: &PresenterState,
                      view: &TuiView| {
            if let Err(e) = term.draw(|f| draw(f, state, view.view_state(), false)) {
                log::debug!("VirtualTui: draw error: {}", e);
            }
        };

        render(&mut terminal, &state, &view);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let rt = match rt {
            Ok(r) => r,
            Err(e) => {
                log::error!("VirtualTui: failed to create runtime: {}", e);
                return;
            }
        };

        let mut input_rx = input_rx;
        let mut event_rx = conn.event_rx;
        let intent_tx = conn.intent_tx;

        while !shutdown.load(Ordering::Relaxed) {
            let mut updated = false;

            while let Ok(ev) = event_rx.try_recv() {
                apply_event(&mut state, &mut view, ev);
                updated = true;
            }

            loop {
                match rt.block_on(tokio::time::timeout(
                    Duration::from_millis(0),
                    input_rx.recv(),
                )) {
                    Ok(Some(bytes)) if !bytes.is_empty() => {
                        input_buf.extend_from_slice(&bytes);
                        while let Some((key, consumed)) = parse_key_from_buf(&mut input_buf) {
                            if let Some(intent) =
                                key_event_to_intent(key, &state.mode, view.view_state())
                            {
                                let _ = intent_tx.send(intent);
                            }
                            input_buf.drain(..consumed);
                        }
                    }
                    Ok(None) => break,
                    _ => break,
                }
            }

            if updated {
                render(&mut terminal, &state, &view);
            }

            thread::sleep(Duration::from_millis(10));
        }
    });
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
                Ok(_) => AppMode::Done,
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
    }
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
}
