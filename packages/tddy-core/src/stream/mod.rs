//! NDJSON stream parsing for agent CLIs (Claude, Cursor) --output-format=stream-json.

pub mod claude;
pub mod cursor;

pub use claude::process_ndjson_stream;

use crate::backend::{ClarificationQuestion, QuestionOption};
use serde::Deserialize;

/// Progress event for real-time display. Each variant has a distinct display representation.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Direct tool use (Read, Bash, Glob, etc.) with optional detail from input.
    ToolUse {
        name: String,
        detail: Option<String>,
    },
    /// Sub-agent task started.
    TaskStarted { description: String },
    /// Sub-agent task progress (e.g. "Running find...", "Reading file").
    TaskProgress {
        description: String,
        last_tool: Option<String>,
    },
    /// Agent session started; session_id from first system/init stream event.
    SessionStarted { session_id: String },
}

/// Result of processing an NDJSON stream from an agent CLI.
///
/// The actual process exit code is obtained from the child process after the stream
/// completes; this struct only contains parsed content from the stream.
#[derive(Debug, Clone)]
pub struct StreamResult {
    pub result_text: String,
    pub session_id: String,
    pub questions: Vec<ClarificationQuestion>,
    /// Raw NDJSON lines from stdout, for debugging when parsing fails.
    pub raw_lines: Vec<String>,
    /// Error messages from result events (e.g. "No conversation found with session ID").
    pub stream_errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AskUserQuestionInput {
    #[serde(default)]
    questions: Vec<AskQuestionItem>,
}

fn default_allow_other() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct AskQuestionItem {
    #[serde(default)]
    question: String,
    #[serde(default)]
    header: String,
    #[serde(default)]
    options: Vec<AskOptionItem>,
    #[serde(default, rename = "multiSelect")]
    multi_select: bool,
    #[serde(default = "default_allow_other", rename = "allowOther")]
    allow_other: bool,
}

#[derive(Debug, Deserialize)]
struct AskOptionItem {
    #[serde(default)]
    label: String,
    #[serde(default)]
    description: String,
}

pub(crate) fn parse_ask_user_question(input: &serde_json::Value) -> Vec<ClarificationQuestion> {
    let parsed: AskUserQuestionInput = match serde_json::from_value(input.clone()) {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    parsed
        .questions
        .into_iter()
        .map(|q| ClarificationQuestion {
            header: q.header,
            question: q.question,
            options: q
                .options
                .into_iter()
                .map(|o| QuestionOption {
                    label: o.label,
                    description: o.description,
                })
                .collect(),
            multi_select: q.multi_select,
            allow_other: q.allow_other,
        })
        .collect()
}
