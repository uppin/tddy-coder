//! Unit tests: the canonical `TokenUsage` arithmetic and the main-agent transcript reader's
//! missing-transcript behavior.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (requirements 3, 5)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md

use tddy_core::token_accounting::{read_main_agent_usage, ConversationRecord, TokenUsage};

/// Total is simply input plus output — the single derived figure shown per conversation.
#[test]
fn token_usage_total_is_input_plus_output() {
    // Given
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 40,
    };

    // When
    let total = usage.total();

    // Then
    assert_eq!(total, 140);
}

/// Accumulating usage adds field-wise, so a session can fold each turn's usage into a running sum.
#[test]
fn token_usage_accumulation_adds_field_wise() {
    // Given
    let first = TokenUsage {
        input_tokens: 100,
        output_tokens: 40,
    };
    let second = TokenUsage {
        input_tokens: 20,
        output_tokens: 5,
    };

    // When
    let combined = first + second;

    // Then
    assert_eq!(
        combined,
        TokenUsage {
            input_tokens: 120,
            output_tokens: 45,
        }
    );
}

/// When Claude wrote no transcript for the session (e.g. it exited before its first turn), the
/// main agent is reported with zero tokens and the model from the session's CLI args — never an
/// error, so the summary still renders a main-agent row.
#[test]
fn read_main_agent_usage_returns_zero_with_the_fallback_model_when_no_transcript_exists() {
    // Given — an empty persistent home (no `.claude/projects/**` transcript).
    let home = tempfile::tempdir().expect("tempdir");

    // When
    let record = read_main_agent_usage(home.path(), "sess-missing", "claude-opus-4-8");

    // Then
    assert_eq!(
        record,
        ConversationRecord {
            agent: "claude".to_string(),
            id: "sess-missing".to_string(),
            model: "claude-opus-4-8".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            turns: 0,
        }
    );
}
