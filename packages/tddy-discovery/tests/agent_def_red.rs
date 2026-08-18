//! Unit tests: specialized-agent YAML definitions (`tddy_discovery::agent_def`).
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criteria 1-4)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md
//!
//! `<tddyhome>/agents/*.yaml` is the on-disk source of truth for specialized subagents; this
//! module is the single loader consumed by the MCP subagent registry, the standalone
//! `tddy-sandbox-app` CLI, and the `tddy-coder` workflow backend — see the crate doc comment on
//! `agent_def.rs`.

use tddy_discovery::agent_def::{load_agent_defs, SpecializedAgentDef};

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
        "name: my-reviewer\nmodel: qwen2.5-coder:14b\nbase_url: http://localhost:11434\n",
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
        "name: good-agent\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
    );
    write_yaml(dir.path(), "broken.yaml", "not: [valid: yaml: at all");
    write_yaml(
        dir.path(),
        "bad-tool.yaml",
        "name: bad-tool-agent\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\ntools: [READ, NOT_A_REAL_TOOL]\n",
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

/// A YAML def without a `replaces:` key deserializes to an empty list — absent means "replaces
/// nothing", not a parse error.
#[test]
fn agent_def_replaces_field_defaults_to_empty_when_absent() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    write_yaml(
        dir.path(),
        "my-explorer.yaml",
        "name: my-explorer\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
    );

    // When
    let defs = load_agent_defs(dir.path());

    // Then
    assert_eq!(defs.len(), 1, "expected exactly one loaded def: {defs:?}");
    assert_eq!(defs[0].replaces, Vec::<String>::new());
}

/// A def that omits `base_url` names no endpoint, and nothing supplies one for it: it fails to
/// load, saying which field is missing. A default here would be one host's port living on in every
/// operator's def, so an agent would resolve and then talk to whatever happened to answer there.
#[test]
fn a_def_that_names_no_endpoint_fails_to_load_rather_than_getting_a_default_one() {
    // Given
    let yaml = "name: my-explorer\nmodel: qwen2.5-coder:7b\n";

    // When
    let result: Result<SpecializedAgentDef, _> = serde_yaml::from_str(yaml);

    // Then
    let error = result
        .expect_err("a def with no base_url must not deserialize")
        .to_string();
    assert!(
        error.contains("base_url"),
        "the failure must name the missing field; got: {error}"
    );
}

/// The same at directory-load level: the endpoint-less def is dropped (the log names its file),
/// and the rest of the directory still loads. Nothing is loaded pointing at a substituted endpoint.
#[test]
fn load_agent_defs_drops_a_def_that_names_no_endpoint() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    write_yaml(
        dir.path(),
        "endpointless.yaml",
        "name: endpointless\nmodel: qwen2.5-coder:7b\n",
    );
    write_yaml(
        dir.path(),
        "explorer.yaml",
        "name: my-explorer\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
    );

    // When
    let defs = load_agent_defs(dir.path());

    // Then
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["my-explorer"],
        "a def naming no endpoint must not load at all; got: {defs:?}"
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
    let yaml = "name: bad-tool-agent\nmodel: qwen2.5-coder:7b\n\
                base_url: http://localhost:11434\ntools: [READ, NOT_A_REAL_TOOL]\n";

    // When
    let result: Result<SpecializedAgentDef, _> = serde_yaml::from_str(yaml);

    // Then
    assert!(
        result.is_err(),
        "an unrecognized tool name must fail deserialization, not silently drop the entry"
    );
}
