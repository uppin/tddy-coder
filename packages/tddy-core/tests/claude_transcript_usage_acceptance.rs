//! Acceptance tests: summing the Claude agent's token usage from its own session transcript.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (acceptance criterion 3, requirement 3)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! This transcript layout (`.claude/projects/<encoded-cwd>/<session_id>.jsonl`) is Claude-Code
//! specific, so the reader lives with the Claude backend (`tddy_core::backend`), not the generic
//! `token_accounting` module.

use std::fs;
use std::path::Path;

use tddy_core::backend::read_claude_transcript_usage;
use tddy_core::token_accounting::ConversationRecord;

/// Write a Claude Code transcript JSONL for `session_id` under the persistent home's
/// `.claude/projects/<encoded-cwd>/` dir, with the given assistant lines already serialized.
fn write_transcript(claude_home: &Path, encoded_cwd: &str, session_id: &str, lines: &[&str]) {
    let dir = claude_home
        .join(".claude")
        .join("projects")
        .join(encoded_cwd);
    fs::create_dir_all(&dir).expect("create projects dir");
    let path = dir.join(format!("{session_id}.jsonl"));
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write transcript");
}

/// The main agent's tokens are the field-wise sum of every assistant message's `message.usage`
/// in the transcript — input and output only (Claude's separate `cache_*` counters are not folded
/// into input), with the model taken from the assistant messages and turns = assistant-line count.
#[test]
fn sums_main_agent_tokens_from_a_claude_transcript_jsonl() {
    // Given — a transcript with two assistant turns (one carrying cache counters that must be
    // ignored) and an interleaved user line that must not be counted.
    let home = tempfile::tempdir().expect("tempdir");
    write_transcript(
        home.path(),
        "-tmp-repo",
        "sess-abc",
        &[
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":900,"cache_creation_input_tokens":30}}}"#,
            r#"{"type":"user","message":{"role":"user","content":"next question"}}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":200,"output_tokens":80}}}"#,
        ],
    );

    // When
    let record = read_claude_transcript_usage(home.path(), "sess-abc", "fallback-model");

    // Then
    assert_eq!(
        record,
        ConversationRecord {
            agent: "claude".to_string(),
            id: "sess-abc".to_string(),
            model: "claude-opus-4-8".to_string(),
            input_tokens: 300,
            output_tokens: 130,
            total_tokens: 430,
            turns: 2,
        }
    );
}

/// When Claude wrote no transcript for the session (e.g. it exited before its first turn), the
/// main agent is reported with zero tokens and the model from the session's CLI args — never an
/// error, so the summary still renders a main-agent row.
#[test]
fn returns_zero_with_the_fallback_model_when_no_transcript_exists() {
    // Given — an empty persistent home (no `.claude/projects/**` transcript).
    let home = tempfile::tempdir().expect("tempdir");

    // When
    let record = read_claude_transcript_usage(home.path(), "sess-missing", "claude-opus-4-8");

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
