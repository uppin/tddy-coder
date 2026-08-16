//! `SubagentTool` covers the **whole exec catalog**, so an assistant assembled in the Models &
//! Agents screen and a subagent declared in YAML speak one tool vocabulary.
//!
//! Before this change `SubagentTool` held six variants (Read/Glob/Grep/Write/StrReplace/Delete)
//! while `tddy_tool_engine::tool_catalog()` held ten, and `SpecializedAgentDef.replaces` bridged the
//! gap with free strings. An assistant cannot be given `Shell` or `SemanticSearch` under that split.
//!
//! The catalog names are spelled out here rather than read from `tddy-tool-engine`: `tddy-discovery`
//! deliberately does not depend on it. The cross-check that this list really is the exec catalog is
//! `names_the_exec_catalog_exactly_as_tddy_discovery_spells_it` in `tddy-daemon`'s
//! `model_registry_store_unit.rs`, which depends on both and asserts the same ten names against
//! `tddy_tool_engine::catalog::tool_catalog()` — so an eleventh engine tool fails that suite rather
//! than silently leaving this list short.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § "The exec catalog is the tool
//! universe".

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The exec-catalog tool names, in the order `tddy_tool_engine::tool_catalog()` lists them.
const EXEC_CATALOG_NAMES: [&str; 10] = [
    "Read",
    "Write",
    "StrReplace",
    "Delete",
    "Grep",
    "Glob",
    "Shell",
    "Await",
    "ReadLints",
    "SemanticSearch",
];

/// A minimal def whose `tools:` list is the thing under test.
fn a_def_with_tools(tools_yaml: &str) -> SpecializedAgentDef {
    let yaml = format!(
        r#"
name: repo-reader
model: qwen3:32b
base_url: http://localhost:11434
tools: {tools_yaml}
"#
    );
    serde_yaml::from_str(&yaml).expect("a specialized agent def")
}

// ---------------------------------------------------------------------------
// The catalog is complete
// ---------------------------------------------------------------------------

#[test]
fn resolves_every_exec_catalog_name_back_to_the_same_name() {
    // Given the exec-catalog tool names
    // When each is resolved to a subagent tool and named again
    let round_tripped: Vec<String> = EXEC_CATALOG_NAMES
        .iter()
        .map(|name| {
            SubagentTool::from_catalog_name(name)
                .unwrap_or_else(|| panic!("exec-catalog tool '{name}' has no SubagentTool variant"))
                .catalog_name()
                .to_string()
        })
        .collect();

    // Then the round trip is the identity — one vocabulary, not two
    assert_eq!(round_tripped, EXEC_CATALOG_NAMES.to_vec());
}

#[test]
fn refuses_a_name_that_is_not_an_exec_catalog_tool() {
    // Given / When
    let resolved = SubagentTool::from_catalog_name("Teleport");

    // Then — an unknown tool is an error, never silently dropped
    assert_eq!(resolved, None);
}

// ---------------------------------------------------------------------------
// YAML round trip
// ---------------------------------------------------------------------------

#[test]
fn accepts_the_four_newly_covered_tools_in_a_yaml_def() {
    // Given a def naming the tools the six-variant enum could not express
    // When
    let def = a_def_with_tools("[SHELL, AWAIT, READ_LINTS, SEMANTIC_SEARCH]");

    // Then
    assert_eq!(
        def.tools,
        vec![
            SubagentTool::Shell,
            SubagentTool::Await,
            SubagentTool::ReadLints,
            SubagentTool::SemanticSearch,
        ]
    );
}

#[test]
fn keeps_accepting_every_tool_existing_yaml_already_declares() {
    // Given a def naming the original six, exactly as a file on disk spells them
    // When
    let def = a_def_with_tools("[READ, GLOB, GREP, WRITE, STR_REPLACE, DELETE]");

    // Then — widening the enum must not break a def already on disk
    assert_eq!(
        def.tools,
        vec![
            SubagentTool::Read,
            SubagentTool::Glob,
            SubagentTool::Grep,
            SubagentTool::Write,
            SubagentTool::StrReplace,
            SubagentTool::Delete,
        ]
    );
}

// ---------------------------------------------------------------------------
// Mutation classification
// ---------------------------------------------------------------------------

#[test]
fn classifies_exactly_the_worktree_changing_tools_as_mutating() {
    // Given every exec-catalog tool
    let all: Vec<SubagentTool> = EXEC_CATALOG_NAMES
        .iter()
        .map(|name| SubagentTool::from_catalog_name(name).expect("an exec-catalog tool"))
        .collect();

    // When partitioned by whether it can change the worktree
    let mutating: Vec<&str> = all
        .iter()
        .filter(|t| t.is_mutating())
        .map(|t| t.catalog_name())
        .collect();

    // Then — Shell joins the three existing mutators; nothing else does
    assert_eq!(mutating, vec!["Write", "StrReplace", "Delete", "Shell"]);
}
