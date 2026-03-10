//! Map crossterm KeyEvent to UserIntent.
//!
//! Requires current AppMode and ViewState to produce the correct intent
//! (e.g. selected index for AnswerSelect, text buffer for SubmitFeatureInput).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::ClarificationQuestion;
use tddy_core::{AppMode, UserIntent};

use crate::view_state::{InboxFocus, ViewState};

/// Map a key event to a UserIntent, if the key has meaning in the current mode.
/// Returns None for keys that are view-local only (e.g. scroll, cursor movement in buffers).
pub fn key_event_to_intent(
    key: KeyEvent,
    mode: &AppMode,
    view_state: &ViewState,
) -> Option<UserIntent> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(UserIntent::Quit);
    }

    match mode {
        AppMode::FeatureInput => feature_input_key(key, view_state),
        AppMode::Running => running_key(key, view_state),
        AppMode::PlanReview { .. } => plan_review_key(key, view_state),
        AppMode::MarkdownViewer { .. } => markdown_viewer_key(key),
        AppMode::Select { question, .. } => select_key(key, question, view_state),
        AppMode::MultiSelect { question, .. } => multiselect_key(key, question, view_state),
        AppMode::TextInput { .. } => text_input_key(key, view_state),
        AppMode::Done => done_key(key),
    }
}

fn plan_review_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    if key.code == KeyCode::Enter {
        match vs.plan_review_selected {
            0 => Some(UserIntent::ViewPlan),
            1 => Some(UserIntent::ApprovePlan),
            2 => Some(UserIntent::RefinePlan),
            _ => None,
        }
    } else {
        None
    }
}

fn markdown_viewer_key(key: KeyEvent) -> Option<UserIntent> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Some(UserIntent::DismissViewer),
        _ => None,
    }
}

fn done_key(key: KeyEvent) -> Option<UserIntent> {
    match key.code {
        KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q') => Some(UserIntent::Quit),
        _ => None,
    }
}

fn feature_input_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    match key.code {
        KeyCode::Enter if !vs.feature_input.is_empty() => {
            Some(UserIntent::SubmitFeatureInput(vs.feature_input.clone()))
        }
        _ => None,
    }
}

fn running_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    match vs.inbox_focus {
        InboxFocus::None => {
            if key.code == KeyCode::Enter && !vs.running_input.is_empty() {
                Some(UserIntent::QueuePrompt(vs.running_input.clone()))
            } else {
                None
            }
        }
        InboxFocus::List => None,
        InboxFocus::Editing => {
            if key.code == KeyCode::Enter {
                Some(UserIntent::EditInboxItem {
                    index: vs.inbox_cursor,
                    text: vs.inbox_edit_buffer.clone(),
                })
            } else if key.code == KeyCode::Char('D') {
                Some(UserIntent::DeleteInboxItem(vs.inbox_cursor))
            } else {
                None
            }
        }
    }
}

fn select_key(
    key: KeyEvent,
    question: &ClarificationQuestion,
    vs: &ViewState,
) -> Option<UserIntent> {
    let option_count = question.options.len();
    let other_idx = option_count;

    match key.code {
        KeyCode::Enter if !vs.select_typing_other && vs.select_selected < other_idx => {
            Some(UserIntent::AnswerSelect(vs.select_selected))
        }
        KeyCode::Enter if !vs.select_typing_other && vs.select_selected == other_idx => {
            Some(UserIntent::AnswerOther(vs.select_other_text.clone()))
        }
        KeyCode::Enter if vs.select_typing_other => {
            Some(UserIntent::AnswerOther(vs.select_other_text.clone()))
        }
        _ => None,
    }
}

fn multiselect_key(
    key: KeyEvent,
    question: &ClarificationQuestion,
    vs: &ViewState,
) -> Option<UserIntent> {
    let other_idx = question.options.len();
    match key.code {
        KeyCode::Enter if !vs.multiselect_typing_other => {
            let indices: Vec<usize> = vs
                .multiselect_checked
                .iter()
                .enumerate()
                .filter(|(i, &c)| *i < other_idx && c)
                .map(|(i, _)| i)
                .collect();
            let other_checked = vs
                .multiselect_checked
                .get(other_idx)
                .copied()
                .unwrap_or(false);
            let other = if other_checked && !vs.multiselect_other_text.is_empty() {
                Some(vs.multiselect_other_text.clone())
            } else {
                None
            };
            Some(UserIntent::AnswerMultiSelect(indices, other))
        }
        KeyCode::Enter if vs.multiselect_typing_other => {
            let indices: Vec<usize> = vs
                .multiselect_checked
                .iter()
                .enumerate()
                .filter(|(i, &c)| *i < other_idx && c)
                .map(|(i, _)| i)
                .collect();
            Some(UserIntent::AnswerMultiSelect(
                indices,
                Some(vs.multiselect_other_text.clone()),
            ))
        }
        _ => None,
    }
}

fn text_input_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    if key.code == KeyCode::Enter && !vs.text_input.is_empty() {
        Some(UserIntent::AnswerText(vs.text_input.clone()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
    }

    #[test]
    fn feature_input_enter_with_text_returns_submit() {
        let mut vs = ViewState::new();
        vs.feature_input = "Build auth".to_string();
        let intent = key_event_to_intent(enter_key(), &AppMode::FeatureInput, &vs);
        assert!(matches!(
            intent,
            Some(UserIntent::SubmitFeatureInput(s)) if s == "Build auth"
        ));
    }

    #[test]
    fn feature_input_enter_empty_returns_none() {
        let vs = ViewState::new();
        let intent = key_event_to_intent(enter_key(), &AppMode::FeatureInput, &vs);
        assert!(intent.is_none());
    }
}
