//! Unit tests: specialized-agent YAML definitions (`tddy_discovery::agent_def`).
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criteria 1-4)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md
//!
//! `<tddyhome>/agents/*.yaml` is the on-disk source of truth for specialized subagents; this
//! module is the single loader consumed by the MCP subagent registry, the standalone
//! `tddy-sandbox-app` CLI, and the `tddy-coder` workflow backend — see the crate doc comment on
//! `agent_def.rs`.

use tddy_discovery::agent_def::{
    builtin_fastcontext_def, load_agent_defs, resolve_agent_defs, SpecializedAgentDef,
};

fn write_yaml(dir: &std::path::Path, filename: &str, contents: &str) {
    std::fs::write(dir.join(filename), contents).expect("write fixture YAML file");
}

/// `load_agent_defs` parses every `*.yaml` file in the directory into a `SpecializedAgentDef`,
/// keyed by the `name` field inside the file (not the file stem).
#[test]
fn load_agent_defs_parses_every_yaml_file_in_the_directory() {
    // Given — two well-formed agent def files
    let dir = tempfile::tempdir().expect("tempdir");
    write_yaml(
        dir.path(),
        "explorer.yaml",
        "name: my-explorer\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
    );
    write_yaml(
        dir.path(),
        "reviewer.yaml",
        "name: my-reviewer\nmodel: qwen2.5-coder:14b\n",
    );

    // When
    let mut defs = load_agent_defs(dir.path());
    defs.sort_by(|a, b| a.name.cmp(&b.name));

    // Then
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["my-explorer", "my-reviewer"],
        "load_agent_defs must return one entry per YAML file, named per the file's own `name` field"
    );
    let explorer = defs.iter().find(|d| d.name == "my-explorer").unwrap();
    assert_eq!(explorer.model, "qwen2.5-coder:7b");
    assert_eq!(explorer.base_url, "http://localhost:11434");
}

/// A malformed file (invalid YAML, or a `tools` entry naming an unrecognized tool) must be
/// skipped — logged, not a panic, and not a silent empty result for the *whole* directory.
#[test]
fn load_agent_defs_skips_a_malformed_file_and_still_loads_the_rest() {
    // Given — one well-formed file, one with invalid YAML, one with an unknown bound tool
    let dir = tempfile::tempdir().expect("tempdir");
    write_yaml(
        dir.path(),
        "good.yaml",
        "name: good-agent\nmodel: qwen2.5-coder:7b\n",
    );
    write_yaml(dir.path(), "broken.yaml", "not: [valid: yaml: at all");
    write_yaml(
        dir.path(),
        "bad-tool.yaml",
        "name: bad-tool-agent\nmodel: qwen2.5-coder:7b\ntools: [READ, NOT_A_REAL_TOOL]\n",
    );

    // When
    let defs = load_agent_defs(dir.path());

    // Then — only the well-formed def survives; loading did not panic and did not return empty
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["good-agent"],
        "malformed files must be skipped, not crash the whole directory load; got: {defs:?}"
    );
}

/// `builtin_fastcontext_def` matches today's shipped defaults exactly (see
/// `packages/tddy-discovery/src/backend.rs`, `packages/tddy-coder/src/run.rs::create_backend`) —
/// zero-config behavior must be unchanged by this generalization.
#[test]
fn builtin_fastcontext_def_matches_todays_shipped_defaults() {
    // When
    let def = builtin_fastcontext_def();

    // Then
    assert_eq!(def.name, "fastcontext");
    assert_eq!(def.model, "microsoft/FastContext-1.0-4B-RL");
    assert_eq!(def.base_url, "http://localhost:30000");
    assert_eq!(def.max_turns, 10);
    assert_eq!(
        def.tools,
        vec![
            tddy_discovery::agent_def::SubagentTool::Read,
            tddy_discovery::agent_def::SubagentTool::Glob,
            tddy_discovery::agent_def::SubagentTool::Grep,
        ]
    );
}

/// A user-defined `<tddyhome>/agents/fastcontext.yaml` (or any def named `fastcontext`) overrides
/// the builtin def of the same name — user config always wins over the shipped default.
#[test]
fn a_user_defined_fastcontext_def_overrides_the_builtin_def_of_the_same_name() {
    // Given — a user def that reuses the "fastcontext" name with a different model/base_url
    let dir = tempfile::tempdir().expect("tempdir");
    write_yaml(
        dir.path(),
        "fastcontext.yaml",
        "name: fastcontext\nmodel: hf.co/mitkox/FastContext-1.0-4B-SFT-Q4_K_M-GGUF:Q4_K_M\nbase_url: http://localhost:11434\n",
    );

    // When
    let defs = resolve_agent_defs(dir.path());

    // Then — exactly one "fastcontext" entry, carrying the user's override values
    let fastcontext_defs: Vec<_> = defs.iter().filter(|d| d.name == "fastcontext").collect();
    assert_eq!(
        fastcontext_defs.len(),
        1,
        "the user override must replace the builtin, not sit alongside it; got: {defs:?}"
    );
    assert_eq!(
        fastcontext_defs[0].model,
        "hf.co/mitkox/FastContext-1.0-4B-SFT-Q4_K_M-GGUF:Q4_K_M",
        "the resolved fastcontext def must carry the user's overridden model, not the builtin default"
    );
}

/// With no `<tddyhome>/agents` overrides at all, `resolve_agent_defs` still surfaces the builtin
/// `fastcontext` def — a fresh install with an empty (or absent) agents directory keeps working.
#[test]
fn resolve_agent_defs_includes_the_builtin_fastcontext_def_when_the_directory_is_empty() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");

    // When
    let defs = resolve_agent_defs(dir.path());

    // Then
    assert!(
        defs.iter().any(|d| d.name == "fastcontext"),
        "resolve_agent_defs must always include the builtin fastcontext def; got: {defs:?}"
    );
}

/// Edge case: `<tddyhome>/agents` not existing at all (not even created) — a brand-new install
/// before the user ever adds a def — must yield an empty list, not a panic or an `Err`. A missing
/// directory is the common case, not an error case.
#[test]
fn load_agent_defs_returns_empty_for_a_directory_that_does_not_exist() {
    // Given — a path that was never created
    let parent = tempfile::tempdir().expect("tempdir");
    let missing_dir = parent.path().join("agents");
    assert!(
        !missing_dir.exists(),
        "precondition: the directory must not exist"
    );

    // When
    let defs = load_agent_defs(&missing_dir);

    // Then
    assert_eq!(
        defs,
        Vec::<SpecializedAgentDef>::new(),
        "a missing <tddyhome>/agents directory must yield an empty list, not panic or error"
    );
}

/// Isolated boundary test for the YAML shape itself (not routed through `load_agent_defs`'s
/// skip-malformed-files behavior): an unrecognized `tools` entry must fail `serde_yaml`
/// deserialization of a lone `SpecializedAgentDef`, proving the rejection is a property of the
/// type's own `Deserialize` impl, not an artifact of directory-scanning.
#[test]
fn specialized_agent_def_yaml_rejects_an_unrecognized_tool_name() {
    // Given
    let yaml = "name: bad-tool-agent\nmodel: qwen2.5-coder:7b\ntools: [READ, NOT_A_REAL_TOOL]\n";

    // When
    let result: Result<SpecializedAgentDef, _> = serde_yaml::from_str(yaml);

    // Then
    assert!(
        result.is_err(),
        "an unrecognized tool name must fail deserialization, not silently drop the entry"
    );
}
