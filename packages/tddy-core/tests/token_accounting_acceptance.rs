//! Acceptance test: the agent-neutral end-of-session summary renderer.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (acceptance criterion 2)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! (The Claude-specific transcript reader is covered by `claude_transcript_usage_acceptance.rs`,
//! since that knowledge lives with the Claude backend, not the generic accounting module.)

use tddy_core::token_accounting::{format_token_summary, ConversationRecord};

/// The end-of-session summary renders one line per conversation (main agent + each subagent) and
/// a TOTAL row that sums the columns — the exact text `tddy-sandbox-app` prints to stderr.
#[test]
fn renders_a_per_agent_breakdown_with_a_total_row() {
    // Given — the main claude agent plus two subagents (one Ollama-backed).
    let records = vec![
        ConversationRecord {
            agent: "claude".to_string(),
            id: "sess-abc".to_string(),
            model: "claude-opus-4-8".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            turns: 3,
        },
        ConversationRecord {
            agent: "fastcontext".to_string(),
            id: "conv-1".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            input_tokens: 120,
            output_tokens: 45,
            total_tokens: 165,
            turns: 2,
        },
        ConversationRecord {
            agent: "codereview".to_string(),
            id: "conv-2".to_string(),
            model: "llama3.1:8b".to_string(),
            input_tokens: 200,
            output_tokens: 80,
            total_tokens: 280,
            turns: 1,
        },
    ];

    // When
    let summary = format_token_summary("sess-abc", &records);

    // Then
    assert_eq!(
        summary,
        "Session token usage (session sess-abc):\n\
         - claude [sess-abc] [claude-opus-4-8]: in=1000 out=500 total=1500 turns=3\n\
         - fastcontext [conv-1] [qwen2.5-coder:7b]: in=120 out=45 total=165 turns=2\n\
         - codereview [conv-2] [llama3.1:8b]: in=200 out=80 total=280 turns=1\n\
         TOTAL: in=1320 out=625 total=1945"
    );
}
