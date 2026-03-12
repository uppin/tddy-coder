//! Main TUI event loop: crossterm events, Presenter polling, rendering.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tddy_core::{Presenter, UserIntent};

use crate::capturing_writer::CapturingWriter;
use crate::key_map::key_event_to_intent;
use crate::raw::{disable_raw_mode, enable_raw_mode_keep_sig};
use crate::render::draw;
use crate::tui_view::TuiView;
use crate::ByteCallback;

/// Run the TUI event loop with the given Presenter.
/// The Presenter must have been started (start_workflow called) before this runs.
/// When `external_intents` is Some, intents from that channel are drained and applied
/// (e.g. from gRPC clients when --grpc is used).
/// When `byte_capture` is Some, all terminal output is captured and passed to the callback
/// (e.g. for gRPC terminal streaming when --grpc is used).
/// When `debug` is true, the debug log area is shown; otherwise it is hidden.
pub fn run_event_loop(
    presenter: &mut Presenter<TuiView>,
    shutdown: &AtomicBool,
    external_intents: Option<mpsc::Receiver<UserIntent>>,
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

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        {
            let state = presenter.state();
            let view = presenter.view();
            terminal.draw(|f| draw(f, state, view.view_state(), debug))?;

            if state.should_quit {
                break;
            }

            // When Done, stay in loop until user presses Enter/Q (sets should_quit).
            // Do not break here — let user review the workflow output first.
        }

        // Drain external intents (e.g. from gRPC clients)
        if let Some(ref rx) = external_intents {
            while let Ok(intent) = rx.try_recv() {
                presenter.handle_intent(intent);
            }
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

                let intent = {
                    let state = presenter.state();
                    let inbox_len = state.inbox.len();
                    let mode = state.mode.clone();
                    let cursor = presenter.view().view_state().inbox_cursor;
                    let edit_item = state.inbox.get(cursor).cloned();

                    let view = presenter.view_mut();
                    let vs = view.view_state_mut();
                    let was_list = matches!(vs.inbox_focus, crate::view_state::InboxFocus::List);
                    let consumed = vs.handle_key_view_local(key, &mode, inbox_len);
                    if was_list
                        && matches!(vs.inbox_focus, crate::view_state::InboxFocus::Editing)
                        && vs.inbox_edit_buffer.is_empty()
                    {
                        vs.inbox_edit_buffer = edit_item.unwrap_or_default();
                    }
                    if consumed {
                        None
                    } else {
                        key_event_to_intent(key, &mode, view.view_state())
                    }
                };
                if let Some(intent) = intent {
                    presenter.handle_intent(intent);
                }
            }
        }

        presenter.poll_tool_calls();
        presenter.poll_workflow();
    }

    execute!(&mut writer_for_execute, LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;

    Ok(())
}
