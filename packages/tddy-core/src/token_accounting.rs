//! Per-conversation token accounting for a session: the canonical record shape shared by the
//! subagent listing / accounting file, the summing of the main `claude` agent's own transcript,
//! and the end-of-session summary `tddy-sandbox-app` prints.
//!
//! Feature: docs/ft/coder/session-token-accounting.md

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Token usage — input and output counts, the only two figures every backend reports uniformly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenUsage {
    /// Total tokens — input plus output.
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl std::ops::Add for TokenUsage {
    type Output = TokenUsage;

    fn add(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
        }
    }
}

/// One conversation's accounting: the main agent or a subagent. This is the wire shape shared by
/// the `subagent_list` MCP tool, the accounting file the in-jail MCP server writes, and the
/// summary renderer — hence the camelCase token fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub agent: String,
    pub id: String,
    pub model: String,
    #[serde(rename = "inputTokens")]
    pub input_tokens: u64,
    #[serde(rename = "outputTokens")]
    pub output_tokens: u64,
    #[serde(rename = "totalTokens")]
    pub total_tokens: u64,
    pub turns: u32,
}

/// Render the per-conversation token breakdown a session prints when it ends: one line per
/// conversation followed by a TOTAL row summing the columns.
pub fn format_token_summary(session_id: &str, records: &[ConversationRecord]) -> String {
    let mut lines = vec![format!("Session token usage (session {session_id}):")];

    let mut total_in = 0u64;
    let mut total_out = 0u64;
    let mut total = 0u64;
    for r in records {
        total_in += r.input_tokens;
        total_out += r.output_tokens;
        total += r.total_tokens;
        lines.push(format!(
            "- {} [{}] [{}]: in={} out={} total={} turns={}",
            r.agent, r.id, r.model, r.input_tokens, r.output_tokens, r.total_tokens, r.turns
        ));
    }

    lines.push(format!(
        "TOTAL: in={total_in} out={total_out} total={total}"
    ));
    lines.join("\n")
}

/// Sum the main `claude` agent's token usage from its transcript.
///
/// The runner spawns `claude --session-id <session_id>`, so Claude Code writes the transcript to
/// `<claude_home>/.claude/projects/<encoded-cwd>/<session_id>.jsonl`. We find it by session id
/// (unique) rather than reconstructing the cwd encoding: for each project subdir, read
/// `<session_id>.jsonl` if present and fold every assistant message's `message.usage`
/// input/output counts (Claude's separate `cache_*` counters are intentionally not folded into
/// input), counting each assistant line as one turn and taking the model from those lines.
///
/// When no transcript exists, the record reports zero tokens with `fallback_model` — never an
/// error, so the summary still renders a main-agent row.
pub fn read_main_agent_usage(
    claude_home: &Path,
    session_id: &str,
    fallback_model: &str,
) -> ConversationRecord {
    let mut usage = TokenUsage::default();
    let mut turns = 0u32;
    let mut model: Option<String> = None;

    let projects_dir = claude_home.join(".claude").join("projects");
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let transcript = entry.path().join(format!("{session_id}.jsonl"));
            let Ok(contents) = std::fs::read_to_string(&transcript) else {
                continue;
            };
            for line in contents.lines() {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                    continue;
                }
                let message = &value["message"];
                usage.input_tokens += message["usage"]["input_tokens"].as_u64().unwrap_or(0);
                usage.output_tokens += message["usage"]["output_tokens"].as_u64().unwrap_or(0);
                if let Some(m) = message["model"].as_str() {
                    model = Some(m.to_string());
                }
                turns += 1;
            }
        }
    }

    ConversationRecord {
        agent: "claude".to_string(),
        id: session_id.to_string(),
        model: model.unwrap_or_else(|| fallback_model.to_string()),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total(),
        turns,
    }
}
