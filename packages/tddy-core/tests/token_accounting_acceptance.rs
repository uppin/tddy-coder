//! Acceptance tests: session token accounting — the canonical `ConversationRecord`, the
//! main-agent transcript reader, and the end-of-session summary renderer.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (acceptance criteria 2-3)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! These pin the two pieces `tddy-sandbox-app` composes at session end: (a) summing the main
//! `claude` agent's tokens out of its own transcript JSONL — the transcript path is deterministic
//! because the runner spawns `claude --session-id <session_id>`; and (b) rendering the merged
//! per-conversation breakdown into the stderr summary.

use std::fs;
use std::path::Path;

use tddy_core::token_accounting::{
    format_token_summary, read_main_agent_usage, ConversationRecord,
};

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
    let record = read_main_agent_usage(home.path(), "sess-abc", "fallback-model");

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
