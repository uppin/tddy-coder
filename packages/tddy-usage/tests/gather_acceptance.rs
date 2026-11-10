//! Acceptance: a session's full snapshot — the agent's own conversations from its binding, then
//! the tddy subagent conversations from `<session_dir>/egress/accounting.json` (`#usage-binding 1/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-claude-transcript.md

mod support;

use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use support::{an_agent_home, the_builtin_binding, the_row, usage};
use tddy_usage::gather_session_usage;

/// Write `<session_dir>/egress/accounting.json` with `conversations`.
fn an_accounting_file(session_dir: &Path, conversations: serde_json::Value) {
    let egress = session_dir.join("egress");
    fs::create_dir_all(&egress).expect("mkdir egress");
    fs::write(
        egress.join("accounting.json"),
        serde_json::json!({ "conversations": conversations }).to_string(),
    )
    .expect("write accounting.json");
}

#[test]
fn tddy_subagent_rows_follow_the_agents_own_rows() {
    // Given
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .assistant_message("msg_01", "claude-opus-4-8", usage(10, 1));
    let session = home.session("sess-1");
    an_accounting_file(
        &session.session_dir,
        serde_json::json!([{
            "agent": "fastcontext", "id": "fc-1", "model": "ollama",
            "inputTokens": 5, "outputTokens": 1, "totalTokens": 6, "turns": 1,
            "cacheReadTokens": 0, "cacheWriteTokens": 0, "reasoningTokens": 0
        }]),
    );

    // When
    let records = gather_session_usage(Some(&the_builtin_binding("claude.yaml")), &session);

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["sess-1", "fc-1"]);
}

#[test]
fn an_accounting_file_written_without_the_new_fields_still_reads() {
    // Given — the seven fields an older tddy-tools writes
    let home = an_agent_home();
    let session = home.session("sess-1");
    an_accounting_file(
        &session.session_dir,
        serde_json::json!([{
            "agent": "fastcontext", "id": "fc-1", "model": "ollama",
            "inputTokens": 5, "outputTokens": 1, "totalTokens": 6, "turns": 1
        }]),
    );

    // When
    let records = gather_session_usage(None, &session);

    // Then
    let row = the_row(&records, "fc-1");
    assert_eq!(
        (
            row.input_tokens,
            row.cache_read_tokens,
            row.cost_usd,
            row.parent_id.clone()
        ),
        (5, 0, None, None)
    );
}

#[test]
fn a_session_whose_agent_declares_no_usage_reports_only_its_tddy_subagents() {
    // Given — a transcript on disk the session's agent does not bind
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .assistant_message("msg_01", "claude-opus-4-8", usage(10, 1));
    let session = home.session("sess-1");
    an_accounting_file(
        &session.session_dir,
        serde_json::json!([{
            "agent": "fastcontext", "id": "fc-1", "model": "ollama",
            "inputTokens": 5, "outputTokens": 1, "totalTokens": 6, "turns": 1
        }]),
    );

    // When
    let records = gather_session_usage(None, &session);

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["fc-1"]);
}
