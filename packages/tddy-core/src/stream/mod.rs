//! NDJSON stream parsing for Claude Code CLI --output-format=stream-json.

use crate::backend::{ClarificationQuestion, QuestionOption};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::BufRead;

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

/// Result of processing an NDJSON stream from Claude.
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
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    result: String,
    #[serde(default)]
    message: Option<AssistantMessage>,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "last_tool_name")]
    last_tool_name: Option<String>,
    /// When set, this assistant message is from a sub-agent; skip ToolUse (task_progress covers it).
    #[serde(default, rename = "parent_tool_use_id")]
    parent_tool_use_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "thinking")]
    Thinking,
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(other)]
    Other,
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

fn parse_ask_user_question(input: &serde_json::Value) -> Vec<ClarificationQuestion> {
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

fn truncate_description(s: &str, max_len: usize) -> String {
    let s = s.trim();
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Extract filename from a `file_path` value for display.
fn file_path_display(obj: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    obj.get("file_path").and_then(|v| v.as_str()).map(|s| {
        std::path::Path::new(s)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(s)
            .to_string()
    })
}

/// Marker for structured output we need to parse (avoids pulling in Read/Bash noise).
const STRUCTURED_RESPONSE_MARKER: &str = "<structured-response";

/// Extract concatenated text from user event's tool_result content when it contains
/// structured-response. Content can be a string or array of {"type":"text","text":"..."} blocks.
/// Only returns content that contains the structured-response marker (avoids Read/Bash output).
fn extract_tool_result_content_from_user_line(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = v.as_object()?;
    if obj.get("type")?.as_str()? != "user" {
        return None;
    }
    let message = obj.get("message")?.as_object()?;
    let content = message.get("content")?.as_array()?;
    let mut out = String::new();
    for block in content {
        let block_obj = block.as_object()?;
        if block_obj.get("type")?.as_str()? != "tool_result" {
            continue;
        }
        let c = block_obj.get("content")?;
        if let Some(s) = c.as_str() {
            if s.contains(STRUCTURED_RESPONSE_MARKER) {
                out.push_str(s);
                if !s.ends_with('\n') {
                    out.push('\n');
                }
            }
        } else if let Some(arr) = c.as_array() {
            for item in arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    if text.contains(STRUCTURED_RESPONSE_MARKER) {
                        out.push_str(text);
                        if !text.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                }
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Extract a short display detail from tool input (file_path, command, description, etc.).
fn tool_use_detail(name: &str, input: &serde_json::Value) -> Option<String> {
    let obj = input.as_object()?;
    let detail = if name == "Read" || name == "Write" {
        file_path_display(obj)
    } else if name == "Bash" {
        obj.get("description")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                obj.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| truncate_description(s, 40))
            })
    } else if name == "Agent" {
        obj.get("description")
            .and_then(|v| v.as_str())
            .map(String::from)
    } else if name == "ToolSearch" {
        obj.get("query")
            .and_then(|v| v.as_str())
            .map(|s| truncate_description(s, 30))
    } else if name == "Glob" {
        obj.get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| truncate_description(s, 40))
    } else {
        None
    };
    detail.filter(|s| !s.is_empty())
}

/// Process NDJSON lines from a reader, extracting result text, session_id, and AskUserQuestion events.
/// Accumulates assistant text and result for parsing. Calls `on_progress` for tool_use, task_started,
/// and task_progress. Optionally calls `on_raw_output` with each text chunk when provided.
pub fn process_ndjson_stream<R, F, O>(
    reader: R,
    mut on_progress: F,
    mut on_raw_output: O,
) -> Result<StreamResult, Box<dyn std::error::Error + Send + Sync>>
where
    R: BufRead,
    F: FnMut(&ProgressEvent),
    O: FnMut(&str),
{
    let mut result_text = String::new();
    let mut session_id = String::new();
    let mut questions: Vec<ClarificationQuestion> = vec![];
    let mut seen_questions: HashSet<(String, String)> = HashSet::new();
    let mut raw_lines: Vec<String> = vec![];

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        raw_lines.push(line.to_string());

        let event: StreamEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Fallback: extract from user tool_result content (Agent tool return, etc.).
        // Claude Code CLI has a known bug where result event can be empty (issue #7124).
        if event.event_type == "user" {
            if let Some(text) = extract_tool_result_content_from_user_line(line) {
                if !text.is_empty() {
                    result_text.push_str(&text);
                    if !text.ends_with('\n') {
                        result_text.push('\n');
                    }
                    on_raw_output(&text);
                    if !text.ends_with('\n') {
                        on_raw_output("\n");
                    }
                }
            }
        }

        match event.event_type.as_str() {
            "system" => {
                if !event.session_id.is_empty() {
                    session_id = event.session_id;
                }
                match event.subtype.as_str() {
                    "task_started" if !event.description.is_empty() => {
                        on_progress(&ProgressEvent::TaskStarted {
                            description: truncate_description(&event.description, 50),
                        });
                    }
                    "task_progress" if !event.description.is_empty() => {
                        on_progress(&ProgressEvent::TaskProgress {
                            description: truncate_description(&event.description, 50),
                            last_tool: event.last_tool_name,
                        });
                    }
                    _ => {}
                }
            }
            "assistant" => {
                let is_subagent = event.parent_tool_use_id.is_some();
                if let Some(msg) = event.message {
                    for block in msg.content {
                        match block {
                            ContentBlock::Text { text } if !text.is_empty() => {
                                result_text.push_str(&text);
                                result_text.push('\n');
                                on_raw_output(&text);
                                if !text.ends_with('\n') {
                                    on_raw_output("\n");
                                }
                            }
                            ContentBlock::ToolUse { name, input } => {
                                if name == "AskUserQuestion" {
                                    for q in parse_ask_user_question(&input) {
                                        let key = (q.header.clone(), q.question.clone());
                                        if seen_questions.insert(key) {
                                            questions.push(q);
                                        }
                                    }
                                } else if !is_subagent {
                                    let detail = tool_use_detail(&name, &input);
                                    on_progress(&ProgressEvent::ToolUse { name, detail });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                if !event.session_id.is_empty() {
                    session_id = event.session_id;
                }
                if !event.result.is_empty() {
                    result_text.push_str(&event.result);
                    if !event.result.ends_with('\n') {
                        result_text.push('\n');
                    }
                    on_raw_output(&event.result);
                    if !event.result.ends_with('\n') {
                        on_raw_output("\n");
                    }
                }
            }
            _ => {}
        }
    }

    Ok(StreamResult {
        result_text: result_text.trim().to_string(),
        session_id,
        questions,
        raw_lines,
    })
}
