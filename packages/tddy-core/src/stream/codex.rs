//! JSONL stream parsing for OpenAI Codex CLI (`codex exec --json`).
//!
//! Event shapes follow Codex CLI JSONL output: `session` lines carry `session_id`;
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
            "session" => {
                if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
                    if session_id.is_none() {
                        session_id = Some(sid.to_string());
                        log::debug!("[tddy-codex] jsonl session_id: {}", sid);
                    }
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
    fn codex_jsonl_parser_fails_malformed_line() {
        let lines = vec!["not-json {{{".to_string()];
        let got = parse_codex_jsonl_output(&lines);
        assert!(
            !got.result_text.is_empty() || got.session_id.is_some(),
            "malformed JSONL should be rejected or surfaced, got empty parse"
        );
    }
}
