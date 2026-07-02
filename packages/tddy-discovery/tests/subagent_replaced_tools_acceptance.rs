//! Acceptance tests: subagent-declared tool replacement.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criteria 13-14)
//! Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md
//!
//! A subagent can declare the exec tools it replaces on the main agent (FastContext replaces
//! `Grep`/`Glob` — its own internal READ/GLOB/GREP loop already covers that ground). The declared
//! default can be overridden per session via a caller-supplied CSV list.

use tddy_discovery::subagent::{resolve_replaced_tools, subagent_replaced_tools};

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
