//! Acceptance tests: subagent-declared tool replacement.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criteria 13-14)
//! Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md
//!
//! A subagent can declare the exec tools it replaces on the main agent (FastContext replaces
//! `Grep`/`Glob` — its own internal READ/GLOB/GREP loop already covers that ground). The declared
//! default can be overridden per session via a caller-supplied CSV list.

use tddy_discovery::agent_def::{builtin_fastcontext_def, SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    normalize_replaced_tools, resolve_replaced_tools, resolve_replaced_tools_for_defs,
    subagent_replaced_tools,
};

fn a_def(name: &str, replaces: &[&str]) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "some-model".to_string(),
        base_url: "http://localhost:30000".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read],
        max_turns: 10,
        replaces: replaces.iter().map(|s| s.to_string()).collect(),
    }
}

// ─── subagent_replaced_tools ────────────────────────────────────────────────────

/// AC13: the built-in `fastcontext` subagent declares `Grep` and `Glob` as replaced — its own
/// internal tool-call loop already covers repository discovery.
#[test]
fn fastcontext_declares_grep_and_glob_as_replaced() {
    // When
    let replaced = subagent_replaced_tools("fastcontext");

    // Then
    assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
}

/// AC13: an unknown subagent name declares nothing replaced — no fabricated tool name, no panic.
#[test]
fn unknown_subagent_name_declares_no_replaced_tools() {
    // When
    let replaced = subagent_replaced_tools("no-such-subagent");

    // Then
    assert_eq!(replaced, Vec::<String>::new());
}

// ─── resolve_replaced_tools ─────────────────────────────────────────────────────

/// AC14: with no override, the declared default for the named subagent applies.
#[test]
fn resolve_uses_the_declared_default_when_no_override_is_given() {
    // When
    let replaced = resolve_replaced_tools("fastcontext", None);

    // Then
    assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
}

/// AC14: an empty override string is treated the same as no override — the default applies.
#[test]
fn resolve_uses_the_declared_default_when_override_is_empty() {
    // When
    let replaced = resolve_replaced_tools("fastcontext", Some(""));

    // Then
    assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
}

/// AC14: a non-empty override replaces the default entirely rather than merging with it.
#[test]
fn resolve_uses_the_override_instead_of_the_default_when_given() {
    // When
    let replaced = resolve_replaced_tools("fastcontext", Some("read"));

    // Then — override wins outright: Read only, Grep/Glob (the default) are not also present
    assert_eq!(replaced, vec!["Read".to_string()]);
}

/// AC14: override tokens are normalized to the exec catalog's canonical casing regardless of how
/// the caller wrote them.
#[test]
fn resolve_normalizes_override_tokens_to_canonical_exec_tool_casing() {
    // When
    let replaced = resolve_replaced_tools("fastcontext", Some("GREP,gLoB"));

    // Then
    assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
}

/// AC14: a token that doesn't match a known exec tool is dropped rather than passed through
/// verbatim — a typo must not silently disable enforcement or invent a tool name.
#[test]
fn resolve_drops_unrecognized_tokens_from_the_override() {
    // When
    let replaced = resolve_replaced_tools("fastcontext", Some("grep,not-a-real-tool"));

    // Then
    assert_eq!(replaced, vec!["Grep".to_string()]);
}

// ─── AC19: single source of truth + array-aware resolution ─────────────────────

/// AC19: `subagent_replaced_tools("fastcontext")` derives its set from
/// `builtin_fastcontext_def().replaces`, not a separate hardcoded literal — this test guards
/// against the two ever drifting apart.
#[test]
fn subagent_replaced_tools_fastcontext_derives_from_builtin_def() {
    // Given
    let expected = normalize_replaced_tools(&builtin_fastcontext_def().replaces);

    // When
    let replaced = subagent_replaced_tools("fastcontext");

    // Then
    assert_eq!(replaced, expected);
}

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
        a_def("fastcontext", &["Grep", "Glob"]),
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
