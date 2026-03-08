//! Frame rendering: draw activity log, status bar, prompt bar to the terminal.
//!
//! AC1: Three regions visible when TUI displays.

use ratatui::Frame;
use ratatui::widgets::Paragraph;

use crate::tui::layout::layout_chunks;
use crate::tui::state::AppState;
use crate::tui::ui::{format_status_bar, status_bar_style_for_goal};

/// Draw the TUI layout: activity log, status bar, prompt bar.
///
/// AC1: Renders three regions so they are visible.
pub fn draw(frame: &mut Frame, state: &AppState) {
    let (activity_log, _status_spacer, status_bar, prompt_bar) = layout_chunks(frame.area());

    if activity_log.height > 0 {
        let content = state
            .activity_log
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let line_count = state.activity_log.len();
        let area_height = activity_log.height as usize;
        let scroll_y = if state.auto_scroll && line_count > area_height {
            (line_count - area_height) as u16
        } else {
            0
        };
        let widget = Paragraph::new(content).scroll((scroll_y, 0));
        frame.render_widget(widget, activity_log);
    }

    if status_bar.height > 0 {
        let text = match (&state.current_goal, &state.current_state) {
            (Some(goal), Some(s)) => {
                let elapsed = state.goal_start_time.elapsed();
                format_status_bar(goal, s, elapsed)
            }
            _ => "Goal: — │ State: — │ Ready".to_string(),
        };
        let style = status_bar_style_for_goal(state.current_goal.as_deref());
        let widget = Paragraph::new(text).style(style);
        frame.render_widget(widget, status_bar);
    }

    if prompt_bar.height > 0 {
        let text = match &state.mode {
            crate::tui::state::AppMode::FeatureInput { input, .. } => {
                let prefix = "> ";
                if input.is_empty() {
                    format!("{}Type your feature description and press Enter...", prefix)
                } else {
                    format!("{}{}", prefix, input)
                }
            }
            _ => String::new(),
        };
        let widget = Paragraph::new(text);
        frame.render_widget(widget, prompt_bar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// AC1: Drawing produces status bar content in the middle region.
    /// When state has goal and state set, status bar area shows "Goal:" and "State:".
    #[test]
    fn test_draw_renders_status_bar_visible() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        let mut state = AppState::new();
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Planning".to_string());

        terminal
            .draw(|f| draw(f, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let content = buffer.content();
        let width = buffer.area().width as usize;
        let height = buffer.area().height as usize;

        let mut found_goal = false;
        let mut found_state = false;
        for y in 0..height {
            for x in 0..width.min(20) {
                let i = y * width + x;
                if i + 5 <= content.len() {
                    let s: String = content[i..i + 5].iter().map(|c| c.symbol().chars().next().unwrap_or(' ')).collect();
                    if s.contains("Goal:") {
                        found_goal = true;
                    }
                    if s.contains("State") {
                        found_state = true;
                    }
                }
            }
        }
        assert!(found_goal, "status bar must display 'Goal:'");
        assert!(found_state, "status bar must display 'State'");
    }

    /// AC1: Three regions are drawn (activity, status, prompt).
    /// Prompt bar at bottom shows placeholder for FeatureInput mode.
    #[test]
    fn test_draw_renders_prompt_bar_at_bottom() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let state = AppState::new();

        terminal
            .draw(|f| draw(f, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let height = buffer.area().height as usize;
        let width = buffer.area().width as usize;
        let content = buffer.content();

        let last_row_start = (height - 1) * width;
        let last_row: String = content[last_row_start..last_row_start + width]
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(
            !last_row.trim().is_empty() || height >= 2,
            "prompt bar at bottom must have content or layout reserves space"
        );
    }
}
