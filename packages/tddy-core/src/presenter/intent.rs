//! Abstract user intents — no KeyEvents reach the Presenter.

/// User actions the Presenter understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIntent {
    /// User submitted feature description (Enter in FeatureInput mode).
    SubmitFeatureInput(String),
    /// User chose the `/recipe` built-in from the feature-prompt slash menu.
    FeatureSlashBuiltinRecipe,
    /// User approved the session document (document review mode).
    ApproveSessionDocument,
    /// User wants to view the session document (document review mode).
    ViewSessionDocument,
    /// User wants to refine the session document (document review mode).
    RefineSessionDocument,
    /// User rejected the session document (document review / markdown viewer) without approving.
    RejectSessionDocument,
    /// User dismissed the markdown viewer (MarkdownViewer mode).
    DismissViewer,
    /// User moved the highlight in Select mode (Up/Down / mouse). Keeps presenter state in sync for
    /// `connect_view` / VirtualTui reconnect snapshots.
    SelectHighlightChanged(usize),
    /// User selected option at index (Select mode). `clarification_question_index` binds the tap to
    /// a specific multi-step clarification step when set (remote / Telegram); `None` is local TUI.
    AnswerSelect {
        option_index: usize,
        clarification_question_index: Option<usize>,
    },
    /// User typed custom answer for "Other" (Select mode).
    AnswerOther(String),
    /// User submitted multi-select answer: checked indices + optional other text.
    AnswerMultiSelect(Vec<usize>, Option<String>),
    /// User submitted free-form text (TextInput mode).
    AnswerText(String),
    /// User queued a prompt during Running mode (Enter on non-empty input).
    QueuePrompt(String),
    /// User edited inbox item at index.
    EditInboxItem { index: usize, text: String },
    /// User deleted inbox item at index.
    DeleteInboxItem(usize),
    /// User scrolled activity log (delta lines).
    Scroll(i32),
    /// User requested quit.
    Quit,
    /// Pointer Stop pane / interrupt — handled in TUI layer (`ctrl_c_interrupt_session`); must not
    /// be sent to the presenter in normal flows (exhaustiveness / tests only).
    Interrupt,
    /// User selected Resume in ErrorRecovery mode.
    ResumeFromError,
    /// User selected "Continue with agent" in ErrorRecovery mode.
    ContinueWithAgent,
}

impl UserIntent {
    /// Select an option for the **current** prompt (TUI / legacy remote without step binding).
    pub fn answer_select(option_index: usize) -> Self {
        UserIntent::AnswerSelect {
            option_index,
            clarification_question_index: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_intent_submit_feature_input() {
        let intent = UserIntent::SubmitFeatureInput("Build auth".to_string());
        assert!(matches!(intent, UserIntent::SubmitFeatureInput(s) if s == "Build auth"));
    }

    #[test]
    fn user_intent_answer_select() {
        let intent = UserIntent::answer_select(2);
        assert!(matches!(
            intent,
            UserIntent::AnswerSelect {
                option_index: 2,
                clarification_question_index: None
            }
        ));
    }

    #[test]
    fn user_intent_queue_prompt() {
        let intent = UserIntent::QueuePrompt("fix bug".to_string());
        assert!(matches!(intent, UserIntent::QueuePrompt(s) if s == "fix bug"));
    }
}
