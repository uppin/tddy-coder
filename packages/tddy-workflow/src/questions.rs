//! The clarification questions a backend asks, and the options it offers for one.
//!
//! Plain serde data with no dependency on the backend abstraction that happens to produce it.
//! `stream` and `toolcall` both need these types and neither needs a backend, which is what made
//! `backend` the wrong home for them.

/// Structured clarification question from AskUserQuestion tool.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClarificationQuestion {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    #[serde(default, alias = "multiSelect")]
    pub multi_select: bool,
    /// When false, omit "Other (type your own)" — e.g. for binary permission (Yes/No).
    #[serde(default = "default_allow_other")]
    pub allow_other: bool,
}

/// Option for a clarification question.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    /// Secondary line in the TUI; omit in JSON when unused (`tddy-tools ask`).
    #[serde(default)]
    pub description: String,
}

fn default_allow_other() -> bool {
    true
}
