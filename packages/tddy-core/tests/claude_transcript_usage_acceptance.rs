//! Acceptance tests: summing the Claude agent's token usage from its own session transcript, and
//! per-subagent usage for the nested Task-tool subagents it spawns.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (acceptance criterion 3, requirements 1, 3)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! This transcript layout is Claude-Code specific, so the readers live with the Claude backend
//! (`tddy_core::backend`), not the generic `token_accounting` module. Claude writes the main
//! thread to `.claude/projects/<encoded-cwd>/<session_id>.jsonl` and each nested Task subagent to
//! `.claude/projects/<encoded-cwd>/<session_id>/subagents/agent-<id>.jsonl` (+ `.meta.json`).

use std::fs;
use std::path::Path;

use tddy_core::backend::{read_claude_subagent_usages, read_claude_transcript_usage};
use tddy_core::token_accounting::ConversationRecord;

/// Write the main-thread transcript JSONL for `session_id` under the persistent home's
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

/// Write one nested Task-tool subagent transcript (`agent-<id>.jsonl`) plus its `.meta.json`
/// (carrying `agentType`) under `.claude/projects/<encoded-cwd>/<session_id>/subagents/`.
fn write_subagent(
    claude_home: &Path,
    encoded_cwd: &str,
    session_id: &str,
    agent_file: &str,
    agent_type: &str,
    lines: &[&str],
) {
    let dir = claude_home
        .join(".claude")
        .join("projects")
        .join(encoded_cwd)
        .join(session_id)
        .join("subagents");
    fs::create_dir_all(&dir).expect("create subagents dir");
    fs::write(
        dir.join(format!("{agent_file}.jsonl")),
        format!("{}\n", lines.join("\n")),
    )
    .expect("write subagent transcript");
    fs::write(
        dir.join(format!("{agent_file}.meta.json")),
        format!(r#"{{"agentType":"{agent_type}","spawnDepth":1}}"#),
    )
    .expect("write subagent meta");
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

/// Each nested Claude Task-tool subagent is accounted as its own conversation: the agent name
/// comes from the subagent's `.meta.json` `agentType`, the id from the `agent-<id>` file stem, and
/// the tokens/model from that subagent's own transcript — sorted by id for a stable summary.
#[test]
fn lists_each_claude_task_subagent_with_its_own_token_usage() {
    // Given — the session spawned two subagents: an Explore (two turns) and a general-purpose
    // (one turn), each written under `<session_id>/subagents/`.
    let home = tempfile::tempdir().expect("tempdir");
    write_subagent(
        home.path(),
        "-tmp-repo",
        "sess-abc",
        "agent-a1",
        "Explore",
        &[
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":100,"output_tokens":50}},"isSidechain":true}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":20,"output_tokens":10}},"isSidechain":true}"#,
        ],
    );
    write_subagent(
        home.path(),
        "-tmp-repo",
        "sess-abc",
        "agent-a2",
        "general-purpose",
        &[
            r#"{"type":"assistant","message":{"model":"claude-haiku-4-5","usage":{"input_tokens":200,"output_tokens":80}},"isSidechain":true}"#,
        ],
    );

    // When
    let records = read_claude_subagent_usages(home.path(), "sess-abc", "fallback-model");

    // Then
    assert_eq!(
        records,
        vec![
            ConversationRecord {
                agent: "Explore".to_string(),
                id: "agent-a1".to_string(),
                model: "claude-opus-4-8".to_string(),
                input_tokens: 120,
                output_tokens: 60,
                total_tokens: 180,
                turns: 2,
            },
            ConversationRecord {
                agent: "general-purpose".to_string(),
                id: "agent-a2".to_string(),
                model: "claude-haiku-4-5".to_string(),
                input_tokens: 200,
                output_tokens: 80,
                total_tokens: 280,
                turns: 1,
            },
        ]
    );
}

/// A session that spawned no Task-tool subagents yields no subagent rows.
#[test]
fn returns_no_subagents_when_the_session_spawned_none() {
    // Given — a main transcript but no `<session_id>/subagents/` dir.
    let home = tempfile::tempdir().expect("tempdir");
    write_transcript(
        home.path(),
        "-tmp-repo",
        "sess-abc",
        &[
            r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":10,"output_tokens":5}}}"#,
        ],
    );

    // When
    let records = read_claude_subagent_usages(home.path(), "sess-abc", "fallback-model");

    // Then
    assert_eq!(records, Vec::<ConversationRecord>::new());
}
