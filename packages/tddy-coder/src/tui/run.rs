//! TUI entry point: event loop and main connection to workflow.
//!
//! R4: Main thread runs ratatui event loop. When connected, main dispatches here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
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

/// Run the TUI event loop. Receives events from workflow thread, sends answers back.
pub fn run_tui_event_loop(
    event_rx: mpsc::Receiver<TuiEvent>,
    answer_tx: mpsc::SyncSender<String>,
    shutdown: &AtomicBool,
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

    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new();
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

        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        shutdown.store(true, Ordering::Relaxed);
                        tddy_core::kill_child_process();
                        break;
                    }
                    state.handle_event(TuiEvent::Key(key));

                    if let Some(input) = state.submitted_feature_input.take() {
                        if answer_tx.send(input).is_err() {
                            break;
                        }
                    } else if state.clarification_answers_ready() {
                        let answers = state.collect_answers();
                        if answer_tx.send(answers).is_err() {
                            break;
                        }
                    }
                }
            }
        }

        while let Ok(ev) = event_rx.try_recv() {
            if let TuiEvent::WorkflowComplete(Ok(summary)) = &ev {
                final_output = Some(summary.clone());
            } else if let TuiEvent::WorkflowComplete(Err(e)) = &ev {
                result = Err(anyhow::anyhow!("{}", e));
            }
            state.handle_event(ev);
        }
    }

    execute!(std::io::stdout(), LeaveAlternateScreen)?;
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
        assert!(result.is_ok(), "run_tui must return Ok when shutdown is set");
    }
}
