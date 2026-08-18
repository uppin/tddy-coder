//! Acceptance tests: subagent-declared tool replacement.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criteria 13-14)
//! Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md
//!
//! A subagent declares the exec tools it replaces on the main agent in its own def (an explorer
//! replacing `Grep`/`Glob` — its internal READ/GLOB/GREP loop already covers that ground). The
//! def is the only source: no name carries a replaced set of its own and nothing overrides one
//! (docs/ft/daemon/session-agent-roster.md § Tool replacement, without behaviour).

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{normalize_replaced_tools, resolve_replaced_tools_for_defs};

fn a_def(name: &str, replaces: &[&str]) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "some-model".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read],
        max_turns: 10,
        replaces: replaces.iter().map(|s| s.to_string()).collect(),
    }
}

// ─── AC19: a def's own `replaces` list is the only source ─────────────────────

/// AC19: `normalize_replaced_tools` canonicalizes mixed-case tokens and silently drops unknown
/// ones (never fabricates a tool name).
#[test]
fn normalize_replaced_tools_canonicalizes_case_and_drops_unknown_tokens() {
    // Given
    let tokens = vec![
        "grep".to_string(),
        "GLOB".to_string(),
        "not-a-real-tool".to_string(),
    ];

    // When
    let normalized = normalize_replaced_tools(&tokens);

    // Then
    assert_eq!(normalized, vec!["Grep".to_string(), "Glob".to_string()]);
}

/// AC19: `resolve_replaced_tools_for_defs` unions each def's own `replaces` list, deduped, in
/// first-occurrence order.
#[test]
fn resolve_replaced_tools_for_defs_unions_and_dedups_across_defs() {
    // Given — two defs, one replacing Grep+Glob, the other replacing Glob+ReadLints
    let defs = vec![
        a_def("explorer", &["Grep", "Glob"]),
        a_def("my-linter", &["Glob", "ReadLints"]),
    ];

    // When
    let replaced = resolve_replaced_tools_for_defs(&defs);

    // Then
    assert_eq!(
        replaced,
        vec![
            "Grep".to_string(),
            "Glob".to_string(),
            "ReadLints".to_string()
        ]
    );
}

/// AC19: an unrecognized token in one def's `replaces` list is dropped, not passed through — a
/// typo in one agent's YAML must not silently produce a nonsense allowlist entry.
#[test]
fn resolve_replaced_tools_for_defs_drops_unrecognized_tokens() {
    // Given
    let defs = vec![a_def("my-linter", &["Grep", "not-a-real-tool"])];

    // When
    let replaced = resolve_replaced_tools_for_defs(&defs);

    // Then
    assert_eq!(replaced, vec!["Grep".to_string()]);
}
