//! The open `BUILD.yaml` config schema: `config.type` selects a handler and the
//! remaining keys are an opaque payload the engine does not model.

use tddy_build::graph::BuildGraph;
use tddy_build::load_build_manifest;
use tddy_build::plugin::PluginRegistry;

#[test]
fn minimal_manifest_defaults_optional_fields() {
    // Given / When
    let manifest = load_build_manifest(
        "schema_version: 1\ntargets:\n  - id: a\n    config:\n      type: script\n      command: [\"true\"]\n",
    )
    .expect("minimal manifest must parse via serde defaults");

    // Then
    let target = &manifest.targets[0];
    assert_eq!(target.name, "", "name defaults to empty");
    assert!(target.deps.is_empty(), "deps default to empty");
    assert!(target.actions.is_empty(), "no explicit actions");
    assert_eq!(
        target.config.as_ref().expect("config present").r#type,
        "script"
    );
}

#[test]
fn config_type_is_separated_from_flattened_fields() {
    // Given / When
    let manifest = load_build_manifest(
        "schema_version: 1\ntargets:\n  - id: app:bin\n    config:\n      type: rust_binary\n      package: app\n      features: [x]\n",
    )
    .expect("manifest must parse");

    // Then
    let config = manifest.targets[0].config.as_ref().expect("config present");
    assert_eq!(config.r#type, "rust_binary");
    assert_eq!(
        config.fields.get("package").and_then(|v| v.as_str()),
        Some("app"),
        "non-`type` keys are preserved for the handler"
    );
    assert!(
        config.fields.get("type").is_none(),
        "the `type` tag must be extracted, not duplicated into the opaque fields"
    );
}

#[test]
fn target_without_config_is_actions_only() {
    // Given / When
    let manifest = load_build_manifest(
        "schema_version: 1\ntargets:\n  - id: t\n    actions:\n      - id: step\n        type: command\n        command: [echo, hi]\n",
    )
    .expect("actions-only manifest must parse");

    // Then
    let target = &manifest.targets[0];
    assert!(
        target.config.is_none(),
        "a target may carry only explicit actions, with no typed config"
    );
    assert_eq!(target.actions.len(), 1);
}

#[test]
fn explicit_actions_precede_lowered_config_action() {
    // Given
    let manifest = load_build_manifest(
        "schema_version: 1\ntargets:\n  - id: t\n    actions:\n      - id: pre\n        type: command\n        command: [\"true\"]\n    config:\n      type: script\n      command: [echo]\n",
    )
    .expect("manifest must parse");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");

    // When
    let actions = graph
        .actions_for("t", &PluginRegistry::new())
        .expect("lower target");

    // Then
    assert_eq!(actions.len(), 2, "explicit action + lowered config action");
    assert_eq!(actions[0].id, "pre", "explicit actions come first");
    assert_eq!(
        actions[1].command,
        vec!["echo".to_string()],
        "the lowered config action follows"
    );
}
