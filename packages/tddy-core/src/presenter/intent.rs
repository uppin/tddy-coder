//! Abstract user intents — no KeyEvents reach the Presenter.

/// User actions the Presenter understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIntent {
    /// User submitted feature description (Enter in FeatureInput mode).
    SubmitFeatureInput(String),
    /// User selected option at index (Select mode).
    AnswerSelect(usize),
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
        let intent = UserIntent::AnswerSelect(2);
        assert!(matches!(intent, UserIntent::AnswerSelect(2)));
    }

    #[test]
    fn user_intent_queue_prompt() {
        let intent = UserIntent::QueuePrompt("fix bug".to_string());
        assert!(matches!(intent, UserIntent::QueuePrompt(s) if s == "fix bug"));
    }
}
