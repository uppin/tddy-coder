//! Main TUI event loop: crossterm events, ViewConnection-based rendering.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tddy_core::ViewConnection;

use crate::capturing_writer::CapturingWriter;
use crate::key_map::key_event_to_intent;
use crate::raw::{disable_raw_mode, enable_raw_mode_keep_sig};
use crate::render::draw;
use crate::tui_view::TuiView;
use crate::virtual_tui::apply_event;
use crate::ByteCallback;

/// Run the TUI event loop with a ViewConnection.
/// The presenter must run in a separate thread (poll_workflow, handle_intent).
/// Local key intents are sent via conn.intent_tx; gRPC clients use the same intent_tx via PresenterHandle.
/// When `byte_capture` is Some, all terminal output is captured and passed to the callback.
pub fn run_event_loop(
    conn: ViewConnection,
    shutdown: &AtomicBool,
    byte_capture: Option<ByteCallback>,
    debug: bool,
) -> anyhow::Result<()> {
    if shutdown.load(Ordering::Relaxed) {
        return Ok(());
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
        original_hook(info);
    }));

    enable_raw_mode_keep_sig()?;

    fn noop(_: &[u8]) {}
    let on_write = byte_capture.unwrap_or_else(|| Box::new(noop) as ByteCallback);
    let mut writer = CapturingWriter::new(on_write);
    execute!(&mut writer, EnterAlternateScreen)?;
    let mut writer_for_execute = writer.clone();
    let backend = CrosstermBackend::new(writer);
    let mut terminal = Terminal::new(backend)?;

    let mut state = conn.state_snapshot;
    let mut view = TuiView::new();
    let mut event_rx = conn.event_rx;
    let intent_tx = conn.intent_tx;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Apply any pending events from broadcast
        while let Ok(ev) = event_rx.try_recv() {
            apply_event(&mut state, &mut view, ev);
        }

        terminal.draw(|f| draw(f, &state, view.view_state_mut(), debug))?;

        if state.should_quit {
            break;
        }

        // Poll crossterm for key events
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                {
                    shutdown.store(true, Ordering::Relaxed);
                    tddy_core::kill_child_process();
                    continue;
                }

                let inbox_len = state.inbox.len();
                let mode = state.mode.clone();
                let cursor = view.view_state().inbox_cursor;
                let edit_item = state.inbox.get(cursor).cloned();

                let vs = view.view_state_mut();
                let was_list = matches!(vs.inbox_focus, crate::view_state::InboxFocus::List);
                let consumed = vs.handle_key_view_local(key, &mode, inbox_len);
                if was_list
                    && matches!(vs.inbox_focus, crate::view_state::InboxFocus::Editing)
                    && vs.inbox_edit_buffer.is_empty()
                {
                    vs.inbox_edit_buffer = edit_item.unwrap_or_default();
                }
                if !consumed {
                    if let Some(intent) = key_event_to_intent(key, &mode, view.view_state()) {
                        let _ = intent_tx.send(intent);
                    }
                }
            }
        }
    }

    execute!(&mut writer_for_execute, LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;

    Ok(())
}
