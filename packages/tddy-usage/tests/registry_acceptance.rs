//! Acceptance: the conversations a session opened besides its main one — the coding subagents the
//! daemon ran for it — are accounted with their own agent's binding and nested under the main
//! conversation; and the rows several sources of one binding yield for one conversation merge
//! (`#usage-binding 3/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-live-tap.md

mod support;

use std::path::Path;

use pretty_assertions::assert_eq;
use support::{an_agent_home, the_row};
use tddy_usage::{
    account, gather_session_usage_for, register_conversation, write_usage_context,
    RegisteredConversation, UsageContextFile, UsageSection,
};

/// A binding reading `<agent_home>/<conversation_id>.jsonl`: each `turn` line's `in` and `out`.
fn a_turn_binding() -> UsageSection {
    serde_json::from_value(serde_json::json!({
        "sources": [{
            "format": "jsonl",
            "main": { "path": "{agent_home}/{conversation_id}.jsonl" },
            "record": {
                "where": [{ "field": "type", "equals": "turn" }],
                "tokens": { "input": "in", "output": "out" }
            }
        }]
    }))
    .expect("a valid binding")
}

/// `home/<conversation>.jsonl` with one turn of `input` / `output` tokens.
fn a_turn_in(home: &Path, conversation: &str, input: u64, output: u64) {
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(
        home.join(format!("{conversation}.jsonl")),
        format!("{{\"type\":\"turn\",\"in\":{input},\"out\":{output}}}\n"),
    )
    .unwrap();
}

/// A session `main` running spec `acme`, its agent's files under `root/main-home`.
fn a_recorded_session(root: &Path) -> std::path::PathBuf {
    let session_dir = root.join("session");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_usage_context(
        &session_dir,
        &UsageContextFile {
            spec_id: "acme".to_string(),
            agent_home: Some(root.join("main-home")),
            conversation_id: Some("main".to_string()),
            worktree: None,
            model: None,
        },
    )
    .unwrap();
    a_turn_in(&root.join("main-home"), "main", 100, 10);
    session_dir
}

fn a_subagent(root: &Path, conversation: &str, label: &str) -> RegisteredConversation {
    RegisteredConversation {
        spec_id: "acme".to_string(),
        conversation_id: conversation.to_string(),
        agent_home: Some(root.join("sub-home")),
        worktree: None,
        parent_id: "main".to_string(),
        agent_label: label.to_string(),
    }
}

fn bindings(spec: &str) -> Option<UsageSection> {
    (spec == "acme").then(a_turn_binding)
}

#[test]
fn a_registered_subagent_is_a_child_of_the_main_conversation() {
    // Given
    let root = tempfile::tempdir().unwrap();
    let session_dir = a_recorded_session(root.path());
    register_conversation(&session_dir, &a_subagent(root.path(), "sub-1", "reviewer")).unwrap();
    a_turn_in(&root.path().join("sub-home"), "sub-1", 20, 2);

    // When
    let records = gather_session_usage_for(&session_dir, &bindings);

    // Then
    let rows: Vec<(&str, &str, Option<&str>, u64)> = records
        .iter()
        .map(|r| {
            (
                r.id.as_str(),
                r.agent.as_str(),
                r.parent_id.as_deref(),
                r.input_tokens,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("main", "acme", None, 100),
            ("sub-1", "reviewer", Some("main"), 20)
        ]
    );
}

#[test]
fn a_conversation_re_registered_on_resume_appears_once() {
    // Given
    let root = tempfile::tempdir().unwrap();
    let session_dir = a_recorded_session(root.path());
    register_conversation(&session_dir, &a_subagent(root.path(), "sub-1", "reviewer")).unwrap();
    register_conversation(
        &session_dir,
        &a_subagent(root.path(), "sub-1", "reviewer (resumed)"),
    )
    .unwrap();
    a_turn_in(&root.path().join("sub-home"), "sub-1", 20, 2);

    // When
    let records = gather_session_usage_for(&session_dir, &bindings);

    // Then
    let subagents: Vec<&str> = records
        .iter()
        .filter(|r| r.parent_id.is_some())
        .map(|r| r.agent.as_str())
        .collect();
    assert_eq!(subagents, vec!["reviewer (resumed)"]);
}

#[test]
fn two_sources_describing_one_conversation_merge_into_one_row() {
    // Given — tokens from the agent's own file, cost from its tapped stream
    let home = an_agent_home();
    a_turn_in(home.path(), "conv-1", 12, 3);
    let session = home.session("conv-1");
    let tapped = session.session_dir.join("usage");
    std::fs::create_dir_all(&tapped).unwrap();
    std::fs::write(
        tapped.join("conv-1.jsonl"),
        "{\"type\":\"result\",\"total_cost_usd\":0.25}\n{\"type\":\"result\",\"total_cost_usd\":0.5}\n",
    )
    .unwrap();
    let binding: UsageSection = serde_json::from_value(serde_json::json!({
        "sources": [
            {
                "format": "jsonl",
                "main": { "path": "{agent_home}/{conversation_id}.jsonl" },
                "record": { "where": [{ "field": "type", "equals": "turn" }],
                            "tokens": { "input": "in", "output": "out" } }
            },
            {
                "format": "jsonl",
                "main": { "path": "{session_dir}/usage/{conversation_id}.jsonl" },
                "record": { "where": [{ "field": "type", "equals": "result" }],
                            "tokens": {}, "cost": "total_cost_usd" }
            }
        ]
    }))
    .unwrap();

    // When
    let records = account(&binding, &session);

    // Then
    assert_eq!(records.len(), 1, "one row per conversation: {records:#?}");
    let row = the_row(&records, "conv-1");
    assert_eq!(
        (row.input_tokens, row.output_tokens, row.cost_usd, row.turns),
        (12, 3, Some(0.75), 1)
    );
}
