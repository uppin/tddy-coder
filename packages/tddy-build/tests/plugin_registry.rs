//! Unit-level coverage of the plugin wiring point: `PluginRegistry` lookup
//! semantics and the `LowerContext` the engine hands to a plugin.

use std::sync::Arc;

use tddy_build::graph::BuildGraph;
use tddy_build::load_build_manifest;
use tddy_build::plugin::{BuildPlugin, LowerContext, PluginRegistry};
use tddy_build::proto::{ActionType, BuildAction};
use tddy_build::BuildError;

/// A plugin claiming a fixed set of `type` names, lowering to a single-argv action
/// whose program is `tag` — lets a test tell two registrations apart.
struct Marker {
    types: &'static [&'static str],
    tag: &'static str,
}

impl BuildPlugin for Marker {
    fn type_names(&self) -> &'static [&'static str] {
        self.types
    }

    fn lower(&self, _ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        Ok(vec![BuildAction {
            id: "marker".to_string(),
            r#type: ActionType::Command as i32,
            command: vec![self.tag.to_string()],
            ..Default::default()
        }])
    }
}

/// Echoes the `LowerContext` it receives into an action command, so a test can assert
/// exactly what the engine passes to a plugin.
struct RecordingPlugin;

impl BuildPlugin for RecordingPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["record"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        Ok(vec![BuildAction {
            id: "rec".to_string(),
            r#type: ActionType::Command as i32,
            command: vec![
                ctx.type_name.to_string(),
                ctx.target_id.to_string(),
                ctx.target_name.to_string(),
                ctx.deps.join(","),
            ],
            ..Default::default()
        }])
    }
}

fn lower_via(registry: &PluginRegistry, type_name: &str) -> Vec<String> {
    let config = serde_yaml::Value::Null;
    let ctx = LowerContext {
        type_name,
        target_id: "t",
        target_name: "",
        deps: &[],
        config: &config,
    };
    registry
        .get(type_name)
        .unwrap_or_else(|| panic!("no plugin registered for {type_name}"))
        .lower(&ctx)
        .expect("lower")
        .remove(0)
        .command
}

#[test]
fn register_maps_each_declared_type_name() {
    // Given
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(Marker {
        types: &["alpha", "beta"],
        tag: "AB",
    }));

    // When / Then
    assert!(registry.get("alpha").is_some(), "alpha must be registered");
    assert!(registry.get("beta").is_some(), "beta must be registered");
    assert_eq!(lower_via(&registry, "alpha"), vec!["AB".to_string()]);
    assert_eq!(lower_via(&registry, "beta"), vec!["AB".to_string()]);
}

#[test]
fn get_returns_none_for_unregistered_type() {
    // Given
    let registry = PluginRegistry::new();

    // When / Then
    assert!(registry.get("missing").is_none());
}

#[test]
fn duplicate_type_registration_last_wins() {
    // Given
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(Marker {
        types: &["x"],
        tag: "first",
    }));
    registry.register(Arc::new(Marker {
        types: &["x"],
        tag: "second",
    }));

    // When / Then
    assert_eq!(
        lower_via(&registry, "x"),
        vec!["second".to_string()],
        "the later registration for a type must win"
    );
}

#[test]
fn registered_types_lists_all_mapped_names() {
    // Given
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(Marker {
        types: &["alpha", "beta"],
        tag: "AB",
    }));

    // When
    let mut names: Vec<String> = registry.registered_types().map(str::to_string).collect();
    names.sort();

    // Then
    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
}

#[test]
fn lower_context_exposes_target_metadata() {
    // Given
    let yaml = r#"
schema_version: 1
targets:
  - id: "dep:one"
    config:
      type: script
      command: ["true"]
  - id: "rec:t"
    name: "Rec Target"
    deps: ["dep:one"]
    config:
      type: record
      foo: bar
"#;
    let manifest = load_build_manifest(yaml).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(RecordingPlugin));

    // When
    let actions = graph
        .actions_for("rec:t", &registry)
        .expect("plugin lowers target");

    // Then
    assert_eq!(
        actions[0].command,
        vec![
            "record".to_string(),     // ctx.type_name
            "rec:t".to_string(),      // ctx.target_id
            "Rec Target".to_string(), // ctx.target_name
            "dep:one".to_string(),    // ctx.deps joined
        ],
        "the engine must pass the target's type, id, name, and deps to the plugin"
    );
}
