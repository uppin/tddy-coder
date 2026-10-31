//! Acceptance: the declared-tool registry — built-ins, `<tddyhome>/tools` overrides, and the
//! refusals that keep a bad tool file from ever reaching a jail.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`).

mod support;

use pretty_assertions::assert_eq;
use support::{a_tool_yaml, a_tools_dir};
use tddy_actions::tool::{HostPatternError, ToolBinary, ToolError, ToolRegistry};

#[test]
fn the_builtin_registry_offers_rg() {
    // Given / When
    let registry = ToolRegistry::builtin().expect("the built-ins load");

    // Then
    assert_eq!(
        registry.get("rg").map(|t| t.binary.clone()),
        Some(ToolBinary {
            command: "rg".into(),
            env: Some("TDDY_RG_BINARY".into()),
        })
    );
}

#[test]
fn an_override_file_replaces_a_builtin_whole_by_id() {
    // Given — an override naming a different binary and no `env`
    let tools = a_tools_dir().with_file("rg.yaml", &a_tool_yaml("rg", "ripgrep-nightly", &[]));

    // When
    let registry = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path())
        .expect("the override loads");

    // Then — nothing of the built-in survives, not even its `env`
    assert_eq!(
        registry.get("rg").map(|t| t.binary.clone()),
        Some(ToolBinary {
            command: "ripgrep-nightly".into(),
            env: None,
        })
    );
    assert_eq!(registry.origin("rg"), Some(tools.file("rg.yaml").as_path()));
}

#[test]
fn an_override_with_a_new_id_adds_a_tool() {
    // Given
    let tools = a_tools_dir().with_file("jq.yaml", &a_tool_yaml("jq", "jq", &[]));

    // When
    let registry = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path())
        .expect("the new tool loads");

    // Then
    assert_eq!(registry.ids().collect::<Vec<_>>(), vec!["jq", "rg"]);
}

#[test]
fn a_missing_overrides_directory_adds_nothing() {
    // Given — a fresh `<tddyhome>` has no `tools/`
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = ToolRegistry::load(ToolRegistry::builtin().unwrap(), &home.path().join("tools"))
        .expect("an absent overrides directory is not an error");

    // Then
    assert_eq!(registry, ToolRegistry::builtin().unwrap());
}

#[test]
fn an_unknown_field_is_refused_naming_the_file_and_the_field() {
    // Given
    let tools = a_tools_dir().with_file(
        "rg.yaml",
        &format!("{}colour: blue\n", a_tool_yaml("rg", "rg", &[])),
    );

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then
    let ToolError::Parse { file, message } = refused else {
        panic!("expected a parse refusal, got {refused:?}");
    };
    assert_eq!(file, tools.file("rg.yaml"));
    // serde_yaml words the rest (the expected-field list and the location); the contract is that
    // the offending field is named.
    assert!(
        message.contains("unknown field `colour`"),
        "the refusal must name the field: {message}"
    );
}

#[test]
fn a_dep_on_an_unknown_tool_is_refused_naming_both_ids() {
    // Given
    let tools = a_tools_dir().with_file("lint.yaml", &a_tool_yaml("lint", "lint", &["node"]));

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then
    assert_eq!(
        refused,
        ToolError::UnknownDep {
            tool: "lint".into(),
            dep: "node".into(),
        }
    );
}

#[test]
fn a_dep_cycle_is_refused_naming_the_cycle() {
    // Given — a → b → c → a
    let tools = a_tools_dir()
        .with_file("a.yaml", &a_tool_yaml("a", "a", &["b"]))
        .with_file("b.yaml", &a_tool_yaml("b", "b", &["c"]))
        .with_file("c.yaml", &a_tool_yaml("c", "c", &["a"]));

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then — listed from the smallest id, closed on it
    assert_eq!(
        refused,
        ToolError::DepCycle {
            cycle: vec!["a".into(), "b".into(), "c".into(), "a".into()],
        }
    );
}

#[test]
fn two_override_files_with_one_id_are_refused_naming_both_files() {
    // Given
    let tools = a_tools_dir()
        .with_file("rg.yaml", &a_tool_yaml("rg", "rg", &[]))
        .with_file("ripgrep.yml", &a_tool_yaml("rg", "rg", &[]));

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then
    assert_eq!(
        refused,
        ToolError::DuplicateId {
            id: "rg".into(),
            first: tools.file("rg.yaml"),
            second: tools.file("ripgrep.yml"),
        }
    );
}

#[test]
fn a_file_naming_another_schema_version_is_refused() {
    // Given
    let tools = a_tools_dir().with_file(
        "rg.yaml",
        &a_tool_yaml("rg", "rg", &[]).replace("schema_version: 1", "schema_version: 2"),
    );

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then
    assert_eq!(
        refused,
        ToolError::Parse {
            file: tools.file("rg.yaml"),
            message: "schema_version 2 is not supported (this build reads 1)".into(),
        }
    );
}

#[test]
fn a_malformed_host_pattern_is_refused_at_load_naming_the_tool() {
    // Given — a tool declaring a URL where a `host[:port]` belongs (`#agent-tools 3/12`)
    let tools = a_tools_dir().with_file(
        "fetch.yaml",
        &format!(
            "{}network:\n  egress: {{ hosts: [\"api.example.com\", \"https://api.example.com/v1\"] }}\n",
            a_tool_yaml("fetch", "curl", &[])
        ),
    );

    // When
    let refused = ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).unwrap_err();

    // Then — the tool's file, and the pattern with why it is not one
    let ToolError::Parse { file, message } = refused else {
        panic!("expected a parse refusal, got {refused:?}");
    };
    assert_eq!(file, tools.file("fetch.yaml"));
    assert!(
        message.contains(
            &HostPatternError {
                pattern: "https://api.example.com/v1".into(),
                reason: "a scheme is not a host".into(),
            }
            .to_string()
        ),
        "the refusal must name the pattern and the reason: {message}"
    );
}
