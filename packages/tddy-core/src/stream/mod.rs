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
}

#[derive(Debug, Deserialize)]
struct AskUserQuestionInput {
    #[serde(default)]
    questions: Vec<AskQuestionItem>,
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
}

#[derive(Debug, Deserialize)]
struct AskOptionItem {
    #[serde(default)]
    label: String,
    #[serde(default)]
    description: String,
}

/// Block marker for structured clarification questions in agent text output.
/// Fallback when the agent outputs questions in text instead of using AskUserQuestion tool.
const CLARIFICATION_QUESTIONS_OPEN: &str = "<clarification-questions";
const CLARIFICATION_QUESTIONS_CLOSE: &str = "</clarification-questions>";

/// Extract clarification questions from agent text when it contains a structured block.
/// Format: <clarification-questions content-type="application-json">{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}</clarification-questions>
/// Used as fallback when Cursor (or other backends) output questions in text instead of tool events.
pub fn parse_clarification_questions_from_text(text: &str) -> Vec<ClarificationQuestion> {
    let Some(open) = text.find(CLARIFICATION_QUESTIONS_OPEN) else {
        return vec![];
    };
    let after_open = &text[open + CLARIFICATION_QUESTIONS_OPEN.len()..];
    let Some(gt) = after_open.find('>') else {
        return vec![];
    };
    let content = after_open[gt + 1..].trim();
    let Some(close) = content.find(CLARIFICATION_QUESTIONS_CLOSE) else {
        return vec![];
    };
    let json_str = content[..close].trim();
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    parse_ask_user_question(&value)
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
        })
        .collect()
}
