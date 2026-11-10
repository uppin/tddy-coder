//! Per-conversation token accounting for a session: the canonical, agent-neutral record shape
//! shared by the subagent listing / accounting file, the usage stream and the end-of-session
//! summary `tddy-sandbox-app` prints.
//!
//! Where an agent's usage comes from is its spec's `usage:` binding, evaluated by
//! [`crate::account`]; nothing here knows one agent from another.
//!
//! Feature: docs/ft/coder/session-token-accounting.md

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
///
/// The counters are disjoint: `input_tokens` is fresh input only (no cache), and `output_tokens`
/// excludes reasoning, so `total_tokens` is their plain sum. Every field after `turns` is defaulted
/// on read, so an accounting file an older writer produced still reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(rename = "cacheReadTokens", default)]
    pub cache_read_tokens: u64,
    #[serde(rename = "cacheWriteTokens", default)]
    pub cache_write_tokens: u64,
    #[serde(rename = "reasoningTokens", default)]
    pub reasoning_tokens: u64,
    /// The cost the source reported, in US dollars. `None` when it reported none.
    #[serde(rename = "costUsd", default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The conversation this one is nested under. `None` for a top-level conversation.
    #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Render the per-conversation token breakdown a session prints when it ends: one line per
/// conversation followed by a TOTAL row summing the columns.
pub fn format_token_summary(session_id: &str, records: &[ConversationRecord]) -> String {
    // TODO(claude-transcript): print cache_read, cache_write and reasoning per row and in TOTAL
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
