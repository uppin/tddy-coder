//! Abstract user intents — no KeyEvents reach the Presenter.

/// User actions the Presenter understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIntent {
    /// User submitted feature description (Enter in FeatureInput mode).
    SubmitFeatureInput(String),
    /// User approved the plan (PlanReview mode).
    ApprovePlan,
    /// User wants to view the PRD (PlanReview mode).
    ViewPlan,
    /// User wants to refine the plan (PlanReview mode).
    RefinePlan,
    /// User dismissed the markdown viewer (MarkdownViewer mode).
    DismissViewer,
    /// User moved the highlight in Select mode (Up/Down). Keeps presenter state in sync for new
    /// `connect_view` / VirtualTui reconnect snapshots.
    SelectHighlightChanged(usize),
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
    /// User selected Resume in ErrorRecovery mode.
    ResumeFromError,
    /// User selected "Continue with agent" in ErrorRecovery mode.
    ContinueWithAgent,
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
