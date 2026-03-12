//! NDJSON stream parsing for Cursor agent --output-format=stream-json.
//!
//! Cursor's event schema differs from Claude's (e.g. tool_call vs tool_use,
//! message.content[0].text, model_call_id skip logic).
//! AskQuestion tool uses askUserQuestionToolCall or askQuestionToolCall with args.questions.

use super::{parse_ask_user_question, ProgressEvent, StreamResult};
use crate::backend::ClarificationQuestion;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::BufRead;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct CursorEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    subtype: String,
    #[serde(default, rename = "session_id")]
    session_id: String,
    #[serde(default, rename = "thread_id")]
    thread_id: String,
    #[serde(default, rename = "threadId")]
    thread_id_camel: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    result: String,
    #[serde(default)]
    message: Option<CursorAssistantMessage>,
    #[serde(default, rename = "model_call_id")]
    #[allow(dead_code)] // Kept for deserialization; no longer used after removing skip logic
    model_call_id: Option<String>,
    #[serde(default)]
    tool_call: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CursorAssistantMessage {
    #[serde(default)]
    content: Vec<CursorContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CursorContentBlock {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Other,
}

fn extract_thread_id(event: &CursorEvent) -> Option<String> {
    for id in [
        &event.session_id,
        &event.thread_id,
        &event.thread_id_camel,
        &event.id,
    ] {
        if !id.is_empty() {
            return Some(id.clone());
        }
    }
    None
}

const MAX_DETAIL_LEN: usize = 40;

fn truncate_detail(s: &str, max_len: usize) -> String {
    let s = s.trim();
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Extract (name, detail) from Cursor tool_call. Structure: { "globToolCall": { "args": { "globPattern": "*.md" } } }
/// or { "readToolCall": { "args": { "path": "/project/README.md" } } }.
fn extract_tool_call_name_and_detail(
    tool_call: &serde_json::Value,
) -> Option<(String, Option<String>)> {
    let obj = tool_call.as_object()?;
    for (key, inner) in obj {
        if !(key.ends_with("ToolCall") || key.ends_with("Tool")) {
            continue;
        }
        let name = key
            .trim_end_matches("ToolCall")
            .trim_end_matches("Tool")
            .to_lowercase();
        let args = inner.get("args")?.as_object()?;
        let detail = if key.to_lowercase().contains("glob") {
            args.get("globPattern")
                .and_then(|v| v.as_str())
                .map(|s| truncate_detail(s, MAX_DETAIL_LEN))
        } else if key.to_lowercase().contains("read") {
            args.get("path").and_then(|v| v.as_str()).map(|s| {
                Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(s)
                    .to_string()
            })
        } else {
            None
        };
        let detail = detail.filter(|s| !s.is_empty());
        return Some((name, detail));
    }
    None
}

/// Process NDJSON lines from Cursor agent stdout.
/// When `on_debug_line` is provided, calls it with each raw NDJSON line (for --debug).
/// When `on_conversation_line` is provided, calls it with each raw line for real-time logging.
/// When `skip_until_line` > 0 (resume), skips calling `on_raw_output` for the first `skip_until_line` lines.
pub fn process_cursor_stream<R, F, O>(
    reader: R,
    mut on_progress: F,
    mut on_raw_output: O,
    mut on_debug_line: Option<&mut dyn FnMut(&str)>,
    mut on_conversation_line: Option<&mut dyn FnMut(&str)>,
    skip_until_line: usize,
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
    let mut line_index: usize = 0;

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        raw_lines.push(line.to_string());
        line_index += 1;
        let should_echo = line_index > skip_until_line;

        if let Some(ref mut f) = on_debug_line {
            f(line);
        }
        if let Some(ref mut f) = on_conversation_line {
            f(line);
        }

        let event: CursorEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                // Not valid JSON — treat as plain text
                if !line.is_empty() {
                    result_text.push_str(line);
                    if should_echo {
                        on_raw_output(line);
                    }
                }
                continue;
            }
        };

        // Capture thread ID from any event
        if session_id.is_empty() {
            if let Some(tid) = extract_thread_id(&event) {
                on_progress(&ProgressEvent::SessionStarted {
                    session_id: tid.clone(),
                });
                session_id = tid;
            }
        }

        match event.event_type.as_str() {
            "assistant" => {
                // Extract text from all assistant messages (deltas and complete).
                // Do NOT skip complete messages (model_call_id) — Cursor may send only the
                // complete message with the structured-response, not incremental deltas.
                if let Some(msg) = event.message {
                    for block in msg.content {
                        if let CursorContentBlock::Text { text } = block {
                            if !text.is_empty() {
                                result_text.push_str(&text);
                                if should_echo {
                                    on_raw_output(&text);
                                }
                            }
                        }
                    }
                }
            }
            "tool_call" => {
                if event.subtype == "started" {
                    if let Some(ref tool_call) = event.tool_call {
                        // Extract AskUserQuestion/AskQuestion for Q&A flow (askUserQuestionToolCall or askQuestionToolCall)
                        let obj = tool_call.as_object();
                        if let Some(obj) = obj {
                            for tool_key in ["askUserQuestionToolCall", "askQuestionToolCall"] {
                                if let Some(inner) = obj.get(tool_key) {
                                    if let Some(args) = inner.get("args") {
                                        for q in parse_ask_user_question(args) {
                                            let dedup_key = (q.header.clone(), q.question.clone());
                                            if seen_questions.insert(dedup_key) {
                                                questions.push(q);
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                        if let Some((name, detail)) = extract_tool_call_name_and_detail(tool_call) {
                            on_progress(&ProgressEvent::ToolUse { name, detail });
                        }
                    }
                }
            }
            "result" => {
                if !event.session_id.is_empty() {
                    if session_id.is_empty() {
                        on_progress(&ProgressEvent::SessionStarted {
                            session_id: event.session_id.clone(),
                        });
                    }
                    session_id = event.session_id.clone();
                }
                if !event.result.is_empty() {
                    result_text.push_str(&event.result);
                    if should_echo {
                        on_raw_output(&event.result);
                    }
                }
            }
            "system" => {
                if !event.session_id.is_empty() && session_id.is_empty() {
                    on_progress(&ProgressEvent::SessionStarted {
                        session_id: event.session_id.clone(),
                    });
                    session_id = event.session_id.clone();
                } else if !event.session_id.is_empty() {
                    session_id = event.session_id.clone();
                }
                if !event.thread_id.is_empty() {
                    if session_id.is_empty() {
                        on_progress(&ProgressEvent::SessionStarted {
                            session_id: event.thread_id.clone(),
                        });
                    }
                    session_id = event.thread_id.clone();
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
        stream_errors: vec![],
    })
}
