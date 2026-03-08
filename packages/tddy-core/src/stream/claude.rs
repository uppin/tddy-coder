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

fn truncate_description(s: &str, max_len: usize) -> String {
    let s = s.trim();
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Marker for structured output we need to parse (avoids pulling in Read/Bash noise).
const STRUCTURED_RESPONSE_MARKER: &str = "<structured-response";

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
    let mut tool_result_text = String::new();
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

        let event: StreamEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Fallback: extract from user tool_result content (Agent tool return, etc.).
        // Claude Code CLI has a known bug where result event can be empty (issue #7124).
        // Collected separately; merged into result_text only when primary sources lack structured-response.
        if event.event_type == "user" {
            if let Some(text) = extract_tool_result_content_from_user_line(line) {
                if !text.is_empty() {
                    tool_result_text.push_str(&text);
                    if should_echo {
                        on_raw_output(&text);
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
            }
            _ => {}
        }
    }

    // Use tool_result as fallback only when primary sources lack structured-response.
    if !result_text.contains(STRUCTURED_RESPONSE_MARKER) && !tool_result_text.is_empty() {
        result_text.push_str(&tool_result_text);
    }

    Ok(StreamResult {
        result_text: result_text.trim().to_string(),
        session_id,
        questions,
        raw_lines,
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
        let result = process_ndjson_stream(reader, noop_progress, noop_output, None, None, 0).unwrap();

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
    fn result_text_uses_tool_result_fallback_when_result_event_empty() {
        let agent_return = concat!(
            "<structured-response content-type=\"application-json\">\n",
            r#"{"goal":"green","summary":"Implemented","tests":[],"implementations":[]}"#,
            "\n",
            "</structured-response>"
        );

        let ndjson = format!(
            "{}\n{}",
            make_user_tool_result(agent_return),
            make_result_event(""),
        );

        let reader = std::io::BufReader::new(ndjson.as_bytes());
        let result = process_ndjson_stream(reader, noop_progress, noop_output, None, None, 0).unwrap();

        assert!(
            result.result_text.contains(r#""goal":"green""#),
            "result_text should fall back to tool_result content when result event is empty"
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
        let result = process_ndjson_stream(reader, noop_progress, noop_output, None, None, 0).unwrap();

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
