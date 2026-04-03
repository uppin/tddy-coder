//! Main TUI event loop: crossterm events, ViewConnection-based rendering.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tddy_core::{AppMode, PresenterView, UserIntent, ViewConnection};

use crate::capturing_writer::CapturingWriter;
use crate::ctrl_interrupt::{ctrl_c_interrupt_session, key_is_ctrl_c_press};
use crate::key_map::key_event_to_intent;
use crate::raw::{disable_raw_mode, enable_raw_mode_keep_sig};
use crate::render::{draw, editing_prompt_cursor_position};
use crate::tui_view::TuiView;
use crate::virtual_tui::drain_presenter_broadcast;
use crate::ByteCallback;

/// Local TUI policy string for editing-mode caret visibility (PRD / crossterm hooks).
///
/// The event loop calls [`crossterm::cursor::Show`] after [`crate::render::draw`] when the prompt
/// exposes a hardware insert position (see [`crate::render::editing_prompt_cursor_position`]).
pub fn local_tui_editing_cursor_policy() -> &'static str {
    log::debug!("local_tui_editing_cursor_policy: visible (Show after draw when caret active)");
    "visible"
}

/// Run the TUI event loop with a ViewConnection.
/// The presenter must run in a separate thread (poll_workflow, handle_intent).
/// Local key intents are sent via conn.intent_tx; gRPC clients use the same intent_tx via PresenterHandle.
/// When `byte_capture` is Some, all terminal output is captured and passed to the callback.
pub fn run_event_loop(
    conn: ViewConnection,
    shutdown: &AtomicBool,
    byte_capture: Option<ByteCallback>,
    debug: bool,
    mouse: bool,
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
    if mouse {
        execute!(&mut writer, EnableMouseCapture)?;
    }
    let mut writer_for_execute = writer.clone();
    let backend = CrosstermBackend::new(writer);
    let mut terminal = Terminal::new(backend)?;

    let mut state = conn.state_snapshot;
    let mut view = TuiView::new();
    view.on_mode_changed(&state.mode);
    let mut event_rx = conn.event_rx;
    let intent_tx = conn.intent_tx;
    let critical_state = conn.critical_state;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Apply any pending events from broadcast (handle Lagged per tokio semantics).
        let _ = drain_presenter_broadcast(&mut event_rx, &mut state, &mut view, &critical_state);

        let mut layout_areas = crate::mouse_map::LayoutAreas {
            activity_log: ratatui::layout::Rect::default(),
            dynamic_area: ratatui::layout::Rect::default(),
            status_bar: ratatui::layout::Rect::default(),
            prompt_bar: ratatui::layout::Rect::default(),
            footer_bar: ratatui::layout::Rect::default(),
            enter_pane: ratatui::layout::Rect::default(),
        };
        terminal.draw(|f| {
            draw(
                f,
                &state,
                view.view_state_mut(),
                debug,
                Some(&mut layout_areas),
            )
        })?;

        if editing_prompt_cursor_position(&state, view.view_state(), layout_areas.prompt_bar)
            .is_some()
        {
            log::trace!("local_tui: editing caret active — crossterm Show");
            let _ = execute!(terminal.backend_mut(), Show);
        }

        if state.should_quit {
            break;
        }

        // Poll crossterm for key and mouse events
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    if key_is_ctrl_c_press(&key) {
                        ctrl_c_interrupt_session(shutdown);
                        continue;
                    }

                    let inbox_len = state.inbox.len();
                    let plan_pending = state.plan_refinement_pending;
                    let mode = state.mode.clone();
                    let skills_root = state.skills_project_root.as_deref();
                    let cursor = view.view_state().inbox_cursor;
                    let edit_item = state.inbox.get(cursor).cloned();

                    let vs = view.view_state_mut();
                    let was_list = matches!(vs.inbox_focus, crate::view_state::InboxFocus::List);
                    let consumed =
                        vs.handle_key_view_local(key, &mode, inbox_len, plan_pending, skills_root);
                    if was_list
                        && matches!(vs.inbox_focus, crate::view_state::InboxFocus::Editing)
                        && vs.inbox_edit_buffer.is_empty()
                    {
                        vs.inbox_edit_buffer = edit_item.unwrap_or_default();
                    }
                    if consumed
                        && matches!(&mode, AppMode::Select { .. })
                        && matches!(key.code, KeyCode::Up | KeyCode::Down)
                    {
                        let idx = view.view_state().select_selected;
                        let _ = intent_tx.send(UserIntent::SelectHighlightChanged(idx));
                    }
                    if view
                        .view_state_mut()
                        .take_pending_feature_slash_builtin_recipe_intent()
                    {
                        let _ = intent_tx.send(UserIntent::FeatureSlashBuiltinRecipe);
                    } else if !consumed {
                        if let Some(intent) =
                            key_event_to_intent(key, &mode, view.view_state(), plan_pending)
                        {
                            let _ = intent_tx.send(intent);
                        }
                    }
                }
                Ok(Event::Mouse(mouse_ev)) if mouse => {
                    let normalized = crate::mouse_map::normalize_mouse_coords_for_local(mouse_ev);
                    if let Some(intent) = crate::mouse_map::handle_mouse_event(
                        normalized,
                        &state.mode,
                        view.view_state_mut(),
                        &layout_areas,
                        state.inbox.len(),
                    ) {
                        let _ = intent_tx.send(intent);
                    }
                    if matches!(state.mode, tddy_core::AppMode::Select { .. }) {
                        let idx = view.view_state().select_selected;
                        let _ = intent_tx.send(tddy_core::UserIntent::SelectHighlightChanged(idx));
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    terminal.clear()?;
                }
                _ => {}
            }
        }
    }

    if mouse {
        execute!(&mut writer_for_execute, DisableMouseCapture)?;
    }
    execute!(&mut writer_for_execute, LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_tui_editing_cursor_policy_is_visible_for_text_edits() {
        assert_eq!(
            super::local_tui_editing_cursor_policy(),
            "visible",
            "PRD: local TUI must expose a visible caret policy for prompt editing modes"
        );
    }
}
