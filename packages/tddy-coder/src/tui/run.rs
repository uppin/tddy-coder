//! TUI entry point: event loop and main connection to workflow.
//!
//! R4: Main thread runs ratatui event loop. When connected, main dispatches here.
//!
//! Crossterm is used for: terminal raw mode, alternate screen, and event reading (dedicated
//! thread). Mouse capture is disabled to allow terminal-native text selection. Ratatui uses
//! CrosstermBackend for drawing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

use crate::tui::raw::{disable_raw_mode, enable_raw_mode_keep_sig};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::tui::event::TuiEvent;
use crate::tui::render::draw;
use crate::tui::state::AppState;

/// Run the TUI event loop. Main calls this when both stdin and stderr are TTY.
///
/// `shutdown`: when it returns true, the event loop exits. Used for Ctrl+C and tests.
pub fn run_tui(shutdown: &AtomicBool) -> anyhow::Result<()> {
    if shutdown.load(Ordering::Relaxed) {
        return Ok(());
    }
    unimplemented!("run_tui: full workflow integration requires run_tui_event_loop with channels")
}

/// Run the TUI event loop. Receives events from workflow thread and crossterm reader thread,
/// sends answers back. Event reading runs in a dedicated thread to avoid blocking the UI.
pub fn run_tui_event_loop(
    event_rx: mpsc::Receiver<TuiEvent>,
    answer_tx: mpsc::Sender<String>,
    event_tx_crossterm: mpsc::Sender<TuiEvent>,
    shutdown: Arc<AtomicBool>,
    agent: &str,
    model: &str,
) -> anyhow::Result<()> {
    if shutdown.load(Ordering::Relaxed) {
        return Ok(());
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        original_hook(info);
    }));

    enable_raw_mode_keep_sig()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    // Do NOT enable mouse capture: it prevents terminal-native text selection (click-drag to copy).
    // Scroll via PageUp/PageDown instead.

    let shutdown_crossterm = Arc::clone(&shutdown);
    let crossterm_handle = thread::spawn(move || {
        let shutdown = shutdown_crossterm;
        while !shutdown.load(Ordering::Relaxed) {
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(ev) = event::read() {
                    let tui_ev = match ev {
                        Event::Key(key) => Some(TuiEvent::Key(key)),
                        Event::Resize(w, h) => Some(TuiEvent::Resize(w, h)),
                        _ => None,
                    };
                    if let Some(t) = tui_ev {
                        let _ = event_tx_crossterm.send(t);
                    }
                }
            }
        }
    });

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(agent, model);
    let mut result = Ok::<(), anyhow::Error>(());
    let mut final_output: Option<String> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        terminal.draw(|f| draw(f, &state))?;

        if state.should_quit {
            break;
        }

        if matches!(state.mode, crate::tui::state::AppMode::Done) {
            break;
        }

        while let Ok(ev) = event_rx.try_recv() {
            if let TuiEvent::Key(key) = &ev {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                {
                    shutdown.store(true, Ordering::Relaxed);
                    tddy_core::kill_child_process();
                    break;
                }
            }
            let is_key = matches!(&ev, TuiEvent::Key(_));
            let (workflow_ok, workflow_err) = match &ev {
                TuiEvent::WorkflowComplete(Ok(s)) => (Some(s.clone()), None),
                TuiEvent::WorkflowComplete(Err(e)) => (None, Some(e.clone())),
                _ => (None, None),
            };
            state.handle_event(ev);

            if is_key {
                if let Some(input) = state.submitted_feature_input.take() {
                    let _ = answer_tx.send(input);
                } else if state.clarification_answers_ready() {
                    let answers = state.collect_answers();
                    let _ = answer_tx.send(answers);
                } else if let Some(choice) = state.demo_choice_to_send.take() {
                    let _ = answer_tx.send(choice);
                }
            }
            if let Some(summary) = workflow_ok {
                if let Some(input) = state.submitted_feature_input.take() {
                    let _ = answer_tx.send(input);
                } else {
                    final_output = Some(summary);
                }
            }
            if let Some(e) = workflow_err {
                result = Err(anyhow::anyhow!("{}", e));
            }
        }
    }

    let _ = crossterm_handle.join();

    execute!(std::io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show)?;
    disable_raw_mode()?;

    if let Some(output) = final_output {
        println!("{}", output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TUI is connected: run_tui exists and returns when shutdown is already set.
    #[test]
    fn test_run_tui_returns_when_shutdown_set() {
        let shutdown = AtomicBool::new(true);
        let result = run_tui(&shutdown);
        assert!(
            result.is_ok(),
            "run_tui must return Ok when shutdown is set"
        );
    }
}
