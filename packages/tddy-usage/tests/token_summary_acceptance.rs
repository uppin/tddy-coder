//! Acceptance: the end-of-session summary `tddy-sandbox-app` prints shows every counter
//! (`#usage-binding 1/5`). Replaces `tddy-agent-backend/tests/token_accounting_acceptance.rs`.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-claude-transcript.md

use pretty_assertions::assert_eq;
use tddy_usage::token_accounting::{format_token_summary, ConversationRecord};

/// A conversation row with every counter set, and nothing else that matters to the summary.
fn a_conversation(
    agent: &str,
    id: &str,
    (input, cache_read, cache_write, output, reasoning): (u64, u64, u64, u64, u64),
) -> ConversationRecord {
    ConversationRecord {
        agent: agent.to_string(),
        id: id.to_string(),
        model: "m".to_string(),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + cache_read + cache_write + output + reasoning,
        turns: 1,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        reasoning_tokens: reasoning,
        cost_usd: None,
        parent_id: None,
    }
}

#[test]
fn the_summary_shows_cache_and_reasoning_per_row_and_in_the_total() {
    // Given
    let records = vec![
        a_conversation("claude", "sess-1", (12, 300, 40, 100, 0)),
        a_conversation("codex", "thr-1", (8, 0, 0, 50, 20)),
    ];

    // When
    let summary = format_token_summary("sess-1", &records);

    // Then
    assert_eq!(
        summary,
        [
            "Session token usage (session sess-1):",
            "- claude [sess-1] [m]: in=12 cache_read=300 cache_write=40 out=100 reasoning=0 total=452 turns=1",
            "- codex [thr-1] [m]: in=8 cache_read=0 cache_write=0 out=50 reasoning=20 total=78 turns=1",
            "TOTAL: in=20 cache_read=300 cache_write=40 out=150 reasoning=20 total=530",
        ]
        .join("\n")
    );
}
