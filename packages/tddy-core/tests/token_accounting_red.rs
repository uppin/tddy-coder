//! Unit tests: the canonical, agent-neutral `TokenUsage` arithmetic.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (requirement 5)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md

use tddy_core::token_accounting::TokenUsage;

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
