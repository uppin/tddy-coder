//! Acceptance: the Claude usage binding shipped in `claude.yaml`, evaluated by the agent-agnostic
//! engine over transcripts shaped the way Claude Code writes them (`#usage-binding 1/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-claude-transcript.md

mod support;

use pretty_assertions::assert_eq;
use support::{an_agent_home, the_builtin_binding, the_row, the_tree, usage};
use tddy_usage::account;

const OPUS: &str = "claude-opus-4-8";
const HAIKU: &str = "claude-haiku-4-5";

#[test]
fn a_message_written_as_several_lines_is_counted_once() {
    // Given
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .user_turn()
        .assistant_message_in_blocks("msg_01", OPUS, usage(12, 2057), 2)
        .assistant_message_in_blocks("msg_02", OPUS, usage(3, 40), 3);

    // When
    let records = account(&the_builtin_binding("claude.yaml"), &home.session("sess-1"));

    // Then
    let main = the_row(&records, "sess-1");
    assert_eq!(
        (main.input_tokens, main.output_tokens, main.turns),
        (15, 2097, 2)
    );
}

#[test]
fn cache_read_and_cache_write_are_reported_beside_fresh_input() {
    // Given
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .assistant_message_in_blocks(
            "msg_01",
            OPUS,
            usage(12, 100).with_cache(332_059, 43_803),
            2,
        );

    // When
    let records = account(&the_builtin_binding("claude.yaml"), &home.session("sess-1"));

    // Then
    let main = the_row(&records, "sess-1");
    assert_eq!(
        (
            main.input_tokens,
            main.cache_read_tokens,
            main.cache_write_tokens,
            main.output_tokens,
            main.total_tokens,
        ),
        (12, 332_059, 43_803, 100, 12 + 332_059 + 43_803 + 100)
    );
}

#[test]
fn a_subagent_of_a_subagent_is_a_child_of_its_parent() {
    // Given
    let home = an_agent_home();
    let project = ".claude/projects/-work-repo";
    home.transcript(&format!("{project}/sess-1.jsonl"))
        .assistant_message("msg_main", OPUS, usage(10, 1));
    home.transcript(&format!("{project}/sess-1/subagents/agent-a.jsonl"))
        .assistant_message("msg_a", HAIKU, usage(20, 2));
    home.transcript(&format!(
        "{project}/sess-1/subagents/agent-a/subagents/agent-b.jsonl"
    ))
    .assistant_message("msg_b", HAIKU, usage(30, 3));

    // When
    let records = account(&the_builtin_binding("claude.yaml"), &home.session("sess-1"));

    // Then
    assert_eq!(
        the_tree(&records),
        vec![
            ("sess-1".to_string(), None),
            ("agent-a".to_string(), Some("sess-1".to_string())),
            ("agent-b".to_string(), Some("agent-a".to_string())),
        ]
    );
    assert_eq!(the_row(&records, "agent-b").input_tokens, 30);
}

#[test]
fn a_subagent_is_named_from_its_sidecar_and_defaults_when_it_has_none() {
    // Given
    let home = an_agent_home();
    let project = ".claude/projects/-work-repo";
    home.transcript(&format!("{project}/sess-1.jsonl"))
        .assistant_message("msg_main", OPUS, usage(10, 1));
    home.transcript(&format!("{project}/sess-1/subagents/agent-a.jsonl"))
        .assistant_message("msg_a", HAIKU, usage(20, 2));
    home.sidecar(
        &format!("{project}/sess-1/subagents/agent-a.meta.json"),
        serde_json::json!({ "agentType": "Explore" }),
    );
    home.transcript(&format!("{project}/sess-1/subagents/agent-b.jsonl"))
        .assistant_message("msg_b", HAIKU, usage(30, 3));

    // When
    let records = account(&the_builtin_binding("claude.yaml"), &home.session("sess-1"));

    // Then
    assert_eq!(
        (
            the_row(&records, "agent-a").agent.as_str(),
            the_row(&records, "agent-b").agent.as_str(),
        ),
        ("Explore", "subagent")
    );
}

#[test]
fn every_transcript_matching_the_main_path_is_merged() {
    // Given — the same session resumed from a second working directory
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .assistant_message("msg_01", OPUS, usage(10, 1));
    home.transcript(".claude/projects/-work-repo-worktree/sess-1.jsonl")
        .assistant_message("msg_02", OPUS, usage(5, 2))
        .garbage_line();

    // When
    let records = account(&the_builtin_binding("claude.yaml"), &home.session("sess-1"));

    // Then
    let main = the_row(&records, "sess-1");
    assert_eq!(
        (
            records.len(),
            main.input_tokens,
            main.output_tokens,
            main.turns,
            main.model.as_str()
        ),
        (1, 15, 3, 2, OPUS)
    );
}

#[test]
fn a_session_with_no_transcript_reports_a_zero_row_with_the_fallback_model() {
    // Given
    let home = an_agent_home();

    // When
    let records = account(
        &the_builtin_binding("claude.yaml"),
        &home.session("sess-missing"),
    );

    // Then
    assert_eq!(records.len(), 1);
    let main = the_row(&records, "sess-missing");
    assert_eq!(
        (
            main.agent.as_str(),
            main.model.as_str(),
            main.total_tokens,
            main.turns,
            main.parent_id.clone()
        ),
        ("claude", "fallback-model", 0, 0, None)
    );
}

#[test]
fn the_claude_acp_spec_reads_the_same_transcripts() {
    // Given
    let home = an_agent_home();
    home.transcript(".claude/projects/-work-repo/sess-1.jsonl")
        .assistant_message_in_blocks("msg_01", OPUS, usage(7, 70).with_cache(700, 0), 2);

    // When
    let records = account(
        &the_builtin_binding("claude-acp.yaml"),
        &home.session("sess-1"),
    );

    // Then
    let main = the_row(&records, "sess-1");
    assert_eq!(
        (
            main.input_tokens,
            main.cache_read_tokens,
            main.output_tokens,
            main.turns
        ),
        (7, 700, 70, 1)
    );
}
