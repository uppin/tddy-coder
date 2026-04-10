//! JSONL stream parsing for OpenAI Codex CLI (`codex exec --json`).
//!
//! Event shapes follow Codex CLI JSONL output: `thread.started` carries `thread_id`;
//! `session` may carry `session_id` (later lines can supersede). Both are used for
//! `codex exec resume <id>`.
//! final assistant-visible text is taken from `item.completed` / `item.text`.

use serde_json::Value;

/// Parsed stdout from a Codex `--json` run (subset of fields needed for [`crate::backend::InvokeResponse`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexStreamResult {
    pub result_text: String,
    pub session_id: Option<String>,
}

/// Parse Codex JSONL lines from stdout.
pub fn parse_codex_jsonl_output(lines: &[String]) -> CodexStreamResult {
    let mut session_id: Option<String> = None;
    let mut result_text = String::new();
    let mut parse_errors: Vec<String> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("[tddy-codex] jsonl line {} not valid JSON: {}", idx + 1, e);
                parse_errors.push(format!("line {}: {}", idx + 1, e));
                continue;
            }
        };

        let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };

        match ty {
            "thread.started" => {
                if session_id.is_none() {
                    if let Some(tid) = v.get("thread_id").and_then(|t| t.as_str()) {
                        session_id = Some(tid.to_string());
                        log::debug!("[tddy-codex] jsonl thread.started thread_id: {}", tid);
                    }
                }
            }
            "session" => {
                if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
                    session_id = Some(sid.to_string());
                    log::debug!("[tddy-codex] jsonl session_id: {}", sid);
                }
            }
            "item.completed" => {
                if let Some(text) = v.pointer("/item/text").and_then(|t| t.as_str()) {
                    result_text = text.to_string();
                    log::debug!(
                        "[tddy-codex] jsonl item.completed ({} bytes)",
                        result_text.len()
                    );
                }
            }
            _ => {
                log::debug!("[tddy-codex] jsonl event type: {}", ty);
            }
        }
    }

    if !parse_errors.is_empty() && result_text.is_empty() && session_id.is_none() {
        result_text = format!("codex jsonl parse error(s): {}", parse_errors.join("; "));
        log::info!(
            "[tddy-codex] jsonl parse had {} error(s), surfaced in result_text",
            parse_errors.len()
        );
    } else if !parse_errors.is_empty() {
        log::info!(
            "[tddy-codex] jsonl parse had {} recoverable error(s) (session or text present)",
            parse_errors.len()
        );
    }

    CodexStreamResult {
        result_text,
        session_id,
    }
}

/// Last `message` from Codex JSONL `error` events, or `/error/message` from `turn.failed`.
/// Prefer this over raw stderr when the CLI exits non-zero — stderr is often noisy tracing.
pub fn codex_jsonl_last_error_message(lines: &[String]) -> Option<String> {
    let mut last: Option<String> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        match ty {
            "error" => {
                if let Some(m) = v.get("message").and_then(|x| x.as_str()) {
                    let s = m.trim();
                    if !s.is_empty() {
                        last = Some(s.to_string());
                    }
                }
            }
            "turn.failed" => {
                if let Some(m) = v.pointer("/error/message").and_then(|x| x.as_str()) {
                    let s = m.trim();
                    if !s.is_empty() {
                        last = Some(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    last
}

/// Strip `tracing-subscriber` style lines (`YYYY-MM-DDTHH:MM:SS.microsZ LEVEL target: msg`) to `msg`.
fn strip_codex_tracing_stderr_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let after_z_level = line
        .find("Z ERROR ")
        .map(|p| &line[p + "Z ERROR ".len()..])
        .or_else(|| line.find("Z WARN ").map(|p| &line[p + "Z WARN ".len()..]))
        .or_else(|| line.find("Z INFO ").map(|p| &line[p + "Z INFO ".len()..]));
    let Some(rest) = after_z_level else {
        return Some(line);
    };
    rest.split_once(": ")
        .map(|(_, msg)| msg.trim())
        .filter(|m| !m.is_empty())
}

/// Compress Codex CLI stderr for user-facing errors (drops repeated tracing prefixes).
pub fn codex_stderr_brief_for_user(stderr: &str) -> Option<String> {
    let mut pieces: Vec<String> = Vec::new();
    for line in stderr.lines() {
        let Some(stripped) = strip_codex_tracing_stderr_line(line) else {
            continue;
        };
        if stripped == "fail" {
            continue;
        }
        if pieces.last().map(|p| p.as_str()) != Some(stripped) {
            pieces.push(stripped.to_string());
        }
    }
    if pieces.is_empty() {
        let t = stderr.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }
    // Last lines usually carry the root cause after retries.
    let tail: Vec<&str> = pieces.iter().rev().take(3).map(String::as_str).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    Some(tail.join(" → "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_jsonl_parser_reads_completed_item_text() {
        let lines = vec![
            r#"{"type":"session","session_id":"codex-sess-xyz"}"#.to_string(),
            r#"{"type":"item.completed","item":{"text":"assistant-visible reply"}}"#.to_string(),
        ];
        let got = parse_codex_jsonl_output(&lines);
        assert_eq!(got.session_id.as_deref(), Some("codex-sess-xyz"));
        assert_eq!(got.result_text, "assistant-visible reply");
    }

    #[test]
    fn codex_jsonl_parser_reads_thread_started_thread_id() {
        let lines = vec![
            r#"{"type":"thread.started","thread_id":"019d73e9-5b90-7ea2-aae8-67aec4c248ed"}"#
                .to_string(),
            r#"{"type":"item.completed","item":{"text":"ok"}}"#.to_string(),
        ];
        let got = parse_codex_jsonl_output(&lines);
        assert_eq!(
            got.session_id.as_deref(),
            Some("019d73e9-5b90-7ea2-aae8-67aec4c248ed")
        );
    }

    #[test]
    fn codex_jsonl_parser_session_overrides_thread_started() {
        let lines = vec![
            r#"{"type":"thread.started","thread_id":"thread-a"}"#.to_string(),
            r#"{"type":"session","session_id":"session-b"}"#.to_string(),
        ];
        let got = parse_codex_jsonl_output(&lines);
        assert_eq!(got.session_id.as_deref(), Some("session-b"));
    }

    #[test]
    fn codex_jsonl_parser_fails_malformed_line() {
        let lines = vec!["not-json {{{".to_string()];
        let got = parse_codex_jsonl_output(&lines);
        assert!(
            !got.result_text.is_empty() || got.session_id.is_some(),
            "malformed JSONL should be rejected or surfaced, got empty parse"
        );
    }

    #[test]
    fn codex_jsonl_last_error_message_prefers_last_turn_failed() {
        let lines = vec![
            r#"{"type":"error","message":"Reconnecting... 1/5"}"#.to_string(),
            r#"{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: no token"}}"#
                .to_string(),
        ];
        assert_eq!(
            codex_jsonl_last_error_message(&lines).as_deref(),
            Some("unexpected status 401 Unauthorized: no token")
        );
    }

    #[test]
    fn codex_stderr_brief_strips_tracing_and_keeps_tail() {
        let stderr =
            "2026-04-09T20:27:23.543178Z ERROR codex_api::endpoint::responses_websocket: a\n\
                  2026-04-09T20:27:24.424463Z ERROR codex_api::endpoint::responses_websocket: a\n\
                  2026-04-09T20:27:25.555500Z ERROR codex_api::endpoint::responses_websocket: b";
        let got = codex_stderr_brief_for_user(stderr).expect("some");
        assert_eq!(got, "a → b");
        assert!(!got.starts_with("2026-"), "got {:?}", got);
    }
}
