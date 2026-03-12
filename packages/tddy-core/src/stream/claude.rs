//! NDJSON stream parsing for Claude Code CLI --output-format=stream-json.
//!
//! Claude's event schema: type (system/assistant/user/result), message.content,
//! tool_use with AskUserQuestion, task_started, task_progress.

use super::{parse_ask_user_question, ProgressEvent, StreamResult};
use crate::backend::ClarificationQuestion;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::BufRead;

#[derive(Debug, Deserialize)]
struct PermissionDenial {
    #[serde(default, rename = "tool_name")]
    tool_name: String,
    #[serde(default, rename = "tool_input")]
    tool_input: Option<serde_json::Value>,
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
    /// When AskUserQuestion is permission-denied, questions appear here instead of tool_use.
    #[serde(default, rename = "permission_denials")]
    permission_denials: Vec<PermissionDenial>,
    /// When subtype is error_during_execution, CLI error messages (e.g. "No conversation found with session ID").
    #[serde(default)]
    errors: Vec<String>,
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

/// Process NDJSON lines from Claude Code CLI stdout.
/// Extracts result text, session_id, and AskUserQuestion events.
/// When `on_conversation_line` is provided, calls it with each raw line for real-time logging.
/// When `skip_until_line` > 0 (resume), skips calling `on_raw_output` for the first `skip_until_line` lines.
pub fn process_ndjson_stream<R, F, O>(
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
    let mut stream_errors: Vec<String> = vec![];
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

        let event: StreamEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match event.event_type.as_str() {
            "system" => {
                if !event.session_id.is_empty() {
                    if session_id.is_empty() {
                        on_progress(&ProgressEvent::SessionStarted {
                            session_id: event.session_id.clone(),
                        });
                    }
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
                                if should_echo {
                                    on_raw_output(&text);
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
                    if should_echo {
                        on_raw_output(&event.result);
                    }
                }
                // When AskUserQuestion is permission-denied, extract questions from permission_denials.
                for denial in &event.permission_denials {
                    if denial.tool_name == "AskUserQuestion" {
                        if let Some(ref input) = denial.tool_input {
                            for q in parse_ask_user_question(input) {
                                let key = (q.header.clone(), q.question.clone());
                                if seen_questions.insert(key) {
                                    questions.push(q);
                                }
                            }
                        }
                    }
                }
                // Collect CLI error messages (e.g. "No conversation found with session ID").
                for err in &event.errors {
                    if !err.is_empty() && !stream_errors.contains(err) {
                        stream_errors.push(err.clone());
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
        stream_errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_progress(_: &ProgressEvent) {}
    fn noop_output(_: &str) {}

    fn make_user_tool_result(content: &str) -> String {
        serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_abc",
                    "content": content
                }]
            },
            "session_id": "sess-1"
        })
        .to_string()
    }

    fn make_assistant_text(text: &str) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": text}]
            },
            "session_id": "sess-1"
        })
        .to_string()
    }

    fn make_result_event(result: &str) -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "result": result,
            "session_id": "sess-1"
        })
        .to_string()
    }

    #[test]
    fn result_text_excludes_tool_result_file_reads_containing_structured_response() {
        let file_content = concat!(
            "pub fn system_prompt() {\n",
            "<structured-response content-type=\"application-json\">\n",
            r#"{"goal":"evaluate-changes","summary":"fake"}"#,
            "\n",
            "</structured-response>\n",
            "}"
        );
        let real_response = concat!(
            "Done.\n",
            "<structured-response content-type=\"application-json\">\n",
            r#"{"goal":"green","summary":"All passing","tests":[],"implementations":[]}"#,
            "\n",
            "</structured-response>"
        );

        let ndjson = format!(
            "{}\n{}\n{}",
            make_user_tool_result(file_content),
            make_assistant_text(real_response),
            make_result_event(real_response),
        );

        let reader = std::io::BufReader::new(ndjson.as_bytes());
        let result =
            process_ndjson_stream(reader, noop_progress, noop_output, None, None, 0).unwrap();

        assert!(
            !result.result_text.contains("evaluate-changes"),
            "result_text must not contain file-read content with fake structured-response"
        );
        assert!(
            result.result_text.contains(r#""goal":"green""#),
            "result_text must contain the real green response"
        );
    }

    #[test]
    fn result_text_prefers_assistant_text_over_tool_result() {
        let file_with_old_response = concat!(
            "<structured-response content-type=\"application-json\">\n",
            r#"{"goal":"red","summary":"Old red output","tests":[],"skeletons":[]}"#,
            "\n",
            "</structured-response>"
        );
        let real_green_text = concat!(
            "<structured-response content-type=\"application-json\">\n",
            r#"{"goal":"green","summary":"New green output","tests":[],"implementations":[]}"#,
            "\n",
            "</structured-response>"
        );

        let ndjson = format!(
            "{}\n{}\n{}",
            make_user_tool_result(file_with_old_response),
            make_assistant_text(real_green_text),
            make_result_event(""),
        );

        let reader = std::io::BufReader::new(ndjson.as_bytes());
        let result =
            process_ndjson_stream(reader, noop_progress, noop_output, None, None, 0).unwrap();

        assert!(
            !result.result_text.contains("Old red output"),
            "result_text must not contain tool_result content when assistant text has structured-response"
        );
        assert!(
            result.result_text.contains("New green output"),
            "result_text must contain the assistant text"
        );
    }
}
