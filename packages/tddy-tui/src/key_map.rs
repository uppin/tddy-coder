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
    plan_refinement_pending: bool,
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
        AppMode::DocumentReview { .. } => document_review_key(key, view_state),
        AppMode::MarkdownViewer { .. } => {
            markdown_viewer_key(key, view_state, plan_refinement_pending)
        }
        AppMode::Select { question, .. } => select_key(key, question, view_state),
        AppMode::MultiSelect { question, .. } => multiselect_key(key, question, view_state),
        AppMode::TextInput { .. } => text_input_key(key, view_state),
        AppMode::Done => done_key(key),
        AppMode::ErrorRecovery { .. } => error_recovery_key(key, view_state),
    }
}

fn document_review_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    if key.code == KeyCode::Enter {
        match vs.document_review_selected {
            0 => Some(UserIntent::ViewSessionDocument),
            1 => Some(UserIntent::ApproveSessionDocument),
            2 => Some(UserIntent::RefineSessionDocument),
            _ => None,
        }
    } else {
        None
    }
}

fn markdown_viewer_key(
    key: KeyEvent,
    vs: &ViewState,
    plan_refinement_pending: bool,
) -> Option<UserIntent> {
    if let Some(intent) = plan_view_approve_reject_shortcuts(key) {
        return Some(intent);
    }
    if !plan_refinement_pending
        && !vs.plan_refinement_input.is_empty()
        && key.code == KeyCode::Enter
    {
        return Some(UserIntent::AnswerText(vs.plan_refinement_input.clone()));
    }
    if plan_refinement_pending {
        if key.code == KeyCode::Enter && !vs.plan_refinement_input.is_empty() {
            log::info!(
                "markdown_viewer_key: submitting refinement ({} chars)",
                vs.plan_refinement_input.len()
            );
            return Some(UserIntent::AnswerText(vs.plan_refinement_input.clone()));
        }
        return None;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Some(UserIntent::DismissViewer),
        KeyCode::Enter => {
            if vs.markdown_at_end {
                match vs.markdown_end_button_selected {
                    0 => Some(UserIntent::ApproveSessionDocument),
                    1 => Some(UserIntent::RefineSessionDocument),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn plan_view_approve_reject_shortcuts(key: KeyEvent) -> Option<UserIntent> {
    if !key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('a') | KeyCode::Char('A') => {
            log::debug!("plan_view_approve_reject_shortcuts: ApproveSessionDocument (Alt+A)");
            Some(UserIntent::ApproveSessionDocument)
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            log::debug!("plan_view_approve_reject_shortcuts: RefineSessionDocument (Alt+R)");
            Some(UserIntent::RefineSessionDocument)
        }
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

fn error_recovery_key(key: KeyEvent, vs: &ViewState) -> Option<UserIntent> {
    log::debug!(
        "error_recovery_key: code={:?} selected={}",
        key.code,
        vs.error_recovery_selected
    );
    if key.code == KeyCode::Enter {
        match vs.error_recovery_selected {
            0 => Some(UserIntent::ResumeFromError),
            1 => Some(UserIntent::ContinueWithAgent),
            _ => Some(UserIntent::Quit),
        }
    } else {
        None
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
        let intent = key_event_to_intent(enter_key(), &AppMode::FeatureInput, &vs, false);
        assert!(matches!(
            intent,
            Some(UserIntent::SubmitFeatureInput(s)) if s == "Build auth"
        ));
    }

    #[test]
    fn feature_input_enter_empty_returns_none() {
        let vs = ViewState::new();
        let intent = key_event_to_intent(enter_key(), &AppMode::FeatureInput, &vs, false);
        assert!(intent.is_none());
    }

    #[test]
    fn enter_at_end_approve_returns_approve_session_document() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 0;
        let mode = AppMode::MarkdownViewer {
            content: "plan content".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(matches!(intent, Some(UserIntent::ApproveSessionDocument)));
    }

    #[test]
    fn enter_at_end_refine_returns_refine_session_document() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 1;
        let mode = AppMode::MarkdownViewer {
            content: "plan content".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(matches!(intent, Some(UserIntent::RefineSessionDocument)));
    }

    #[test]
    fn enter_when_not_at_end_returns_none() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = false;
        let mode = AppMode::MarkdownViewer {
            content: "plan content".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(intent.is_none());
    }

    #[test]
    fn test_error_recovery_key_resume() {
        let mut vs = ViewState::new();
        vs.error_recovery_selected = 0;
        let mode = AppMode::ErrorRecovery {
            error_message: "some error".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(matches!(intent, Some(UserIntent::ResumeFromError)));
    }

    #[test]
    fn test_error_recovery_key_continue_with_agent() {
        let mut vs = ViewState::new();
        vs.error_recovery_selected = 1;
        let mode = AppMode::ErrorRecovery {
            error_message: "some error".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(matches!(intent, Some(UserIntent::ContinueWithAgent)));
    }

    #[test]
    fn test_error_recovery_key_exit() {
        let mut vs = ViewState::new();
        vs.error_recovery_selected = 2;
        let mode = AppMode::ErrorRecovery {
            error_message: "some error".to_string(),
        };
        let intent = key_event_to_intent(enter_key(), &mode, &vs, false);
        assert!(matches!(intent, Some(UserIntent::Quit)));
    }

    #[test]
    fn markdown_viewer_approval_and_refine_use_alt_modifier() {
        let vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "# PRD".to_string(),
        };
        let plain_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(
            key_event_to_intent(plain_a, &mode, &vs, false).is_none(),
            "plain a is for typing in the prompt, not approve"
        );
        let plain_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::empty());
        assert!(
            key_event_to_intent(plain_r, &mode, &vs, false).is_none(),
            "plain r is for typing in the prompt, not refine"
        );
        let alt_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
        assert_eq!(
            key_event_to_intent(alt_a, &mode, &vs, false),
            Some(UserIntent::ApproveSessionDocument)
        );
        let alt_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT);
        assert_eq!(
            key_event_to_intent(alt_r, &mode, &vs, false),
            Some(UserIntent::RefineSessionDocument)
        );
    }
}
