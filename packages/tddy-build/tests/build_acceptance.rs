//! Acceptance tests for the tddy-build crate.
//!
//! These pin the externally-observable contract after `tddy-build` became a generic
//! engine + plugin wiring point: an open `BUILD.yaml` config schema, dispatch of
//! ecosystem target types to registered `BuildPlugin`s, the built-in structural types
//! (`script`/`tool`/`group`), the content-addressed action cache, and DAG execution.
//!
//! `tddy-build` carries no recipe knowledge and depends on no plugin crate, so the
//! plugin path is exercised here via an inline test-only `DemoPlugin`.

use std::collections::HashMap;
use std::sync::Arc;

use tddy_build::cache::compute_cache_key;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::manifest::TargetConfig;
use tddy_build::plugin::{BuildPlugin, LowerContext, PluginRegistry};
use tddy_build::proto::{ActionType, BuildAction, FileFingerprint};
use tddy_build::service::{build_list_json, BuildListQuery};
use tddy_build::{load_build_manifest, BuildError};

/// A test-only plugin handling `type: demo`. It lowers to a single command action
/// `["demo-tool", <message>]`, reading `message` from the target's open config. This
/// stands in for the real recipe crates (`tddy-build-rust`, …) which `tddy-build`
/// must not depend on.
struct DemoPlugin;

impl BuildPlugin for DemoPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["demo"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let message = ctx
            .config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        Ok(vec![BuildAction {
            id: "demo".to_string(),
            r#type: ActionType::Command as i32,
            command: vec!["demo-tool".to_string(), message],
            ..Default::default()
        }])
    }
}

fn registry_with_demo() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(DemoPlugin));
    registry
}

/// A manifest mixing the three built-in structural types (`script`/`tool`/`group`)
/// with a plugin-provided type (`demo`).
const MIXED_YAML: &str = r#"
schema_version: 1
targets:
  - id: "foo:script"
    name: "Script"
    config:
      type: script
      command: [echo, hello]
  - id: "demo:thing"
    name: "Demo Thing"
    config:
      type: demo
      message: hi-from-demo
  - id: "tools:bin"
    name: "Tools"
    config:
      type: tool
      bin_dir: tools/bin
      commands: { greet: greet }
  - id: "all:group"
    name: "Group"
    config:
      type: group
      member_ids: ["foo:script", "demo:thing"]
"#;

#[test]
fn manifest_round_trips_builtin_and_plugin_configs() {
    // When
    let manifest = load_build_manifest(MIXED_YAML).expect("manifest must parse");

    // Then
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.targets.len(), 4);

    let config = |id: &str| -> &TargetConfig {
        manifest
            .targets
            .iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("missing target {id}"))
            .config
            .as_ref()
            .unwrap_or_else(|| panic!("target {id} has no config"))
    };

    // `type` is captured as the dispatch tag; the engine knows nothing more.
    assert_eq!(config("foo:script").r#type, "script");
    assert_eq!(config("tools:bin").r#type, "tool");
    assert_eq!(config("all:group").r#type, "group");
    assert_eq!(config("demo:thing").r#type, "demo");

    // The remaining config keys are preserved verbatim for the plugin to interpret —
    // the engine does not model them.
    assert_eq!(
        config("demo:thing")
            .fields
            .get("message")
            .and_then(|v| v.as_str()),
        Some("hi-from-demo"),
        "plugin config fields must round-trip for the plugin to read"
    );
}

#[test]
fn registered_plugin_lowers_target_actions() {
    // Given
    let manifest = load_build_manifest(MIXED_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let registry = registry_with_demo();

    // When
    let actions = graph
        .actions_for("demo:thing", &registry)
        .expect("registered plugin must lower its target");

    // Then
    assert_eq!(actions.len(), 1, "demo plugin lowers to one action");
    assert_eq!(
        actions[0].command,
        vec!["demo-tool".to_string(), "hi-from-demo".to_string()],
        "the action argv must come from the plugin, not the engine"
    );
}

#[test]
fn unknown_target_type_without_plugin_errors() {
    // Given
    let manifest = load_build_manifest(MIXED_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let empty = PluginRegistry::new(); // no `demo` plugin registered

    // When
    let err = graph
        .actions_for("demo:thing", &empty)
        .expect_err("an unregistered, non-built-in type must error");
    let message = err.to_string();

    // Then
    assert!(
        message.contains("unknown target type"),
        "error must explain the cause, got: {message}"
    );
    assert!(
        message.contains("demo"),
        "error must name the offending type, got: {message}"
    );
}

#[test]
fn builtin_script_tool_group_lower_without_any_plugin() {
    // Given
    let manifest = load_build_manifest(MIXED_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let empty = PluginRegistry::new();

    // When / Then — `script` is the engine's generic command escape hatch.
    let script = graph
        .actions_for("foo:script", &empty)
        .expect("script is built-in");
    assert_eq!(script.len(), 1);
    assert_eq!(
        script[0].command,
        vec!["echo".to_string(), "hello".to_string()]
    );

    // `tool` and `group` are structural — no own build action.
    assert!(
        graph
            .actions_for("tools:bin", &empty)
            .expect("tool is built-in")
            .is_empty(),
        "tool target has no own action"
    );
    assert!(
        graph
            .actions_for("all:group", &empty)
            .expect("group is built-in")
            .is_empty(),
        "group target has no own action"
    );
}

#[test]
fn group_membership_drives_build_order_without_plugins() {
    // Given
    let manifest = load_build_manifest(MIXED_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");

    // When
    let order = graph
        .build_order("all:group")
        .expect("group build order resolves");
    let pos = |id: &str| {
        order
            .iter()
            .position(|t| t == id)
            .unwrap_or_else(|| panic!("{id} missing from build order: {order:?}"))
    };

    // Then
    assert!(
        pos("foo:script") < pos("all:group"),
        "group members build before the group"
    );
    assert!(
        pos("demo:thing") < pos("all:group"),
        "group members build before the group"
    );
}

#[tokio::test]
async fn tool_bin_dir_is_prepended_to_action_path() {
    // Given
    let yaml = r#"
schema_version: 1
targets:
  - id: "mytool:tool"
    name: "My Tool"
    config:
      type: tool
      bin_dir: toolbin
      commands: { greet: greet }
  - id: "uses:tool"
    name: "Uses Tool"
    deps: ["mytool:tool"]
    actions:
      - id: "run-greet"
        description: "invoke the tool-provided binary"
        type: command
        command: ["greet"]
        tool_dep_ids: ["mytool:tool"]
"#;
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    let bin_dir = root.join("toolbin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir toolbin");
    let greet = bin_dir.join("greet");
    std::fs::write(&greet, "#!/bin/sh\necho GREETED-BY-TOOL\n").expect("write greet");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&greet, std::fs::Permissions::from_mode(0o755))
            .expect("chmod greet");
    }
    let manifest = load_build_manifest(yaml).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");

    // When
    let record = execute_target(
        root,
        &graph,
        "uses:tool",
        &ExecuteOptions::default(),
        tddy_build::BuildMode::Compile,
        &PluginRegistry::new(),
    )
    .await
    .expect("run tool-dependent target");

    // Then
    assert!(
        record.actions[0].stdout.contains("GREETED-BY-TOOL"),
        "built-in tool bin_dir must be on PATH, got: {:?}",
        record.actions[0].stdout
    );
}

#[test]
fn service_list_reports_raw_type_string_for_plugin_target() {
    // Given
    let repo = tempfile::tempdir().expect("tempdir");
    std::fs::write(repo.path().join("BUILD.yaml"), MIXED_YAML).expect("write BUILD.yaml");

    // When — listing reads the `type` tag directly — it neither lowers nor needs a registry.
    let value = build_list_json(repo.path(), &BuildListQuery::default()).expect("list targets");
    let targets = value
        .get("targets")
        .and_then(|t| t.as_array())
        .expect("list output must have a `targets` array");
    let demo = targets
        .iter()
        .find(|t| t.get("id").and_then(|v| v.as_str()) == Some("demo:thing"))
        .expect("demo:thing must be listed");

    // Then
    assert_eq!(
        demo.get("type").and_then(|v| v.as_str()),
        Some("demo"),
        "the listed type must be the raw config type string"
    );
}

#[test]
fn build_manifest_rejects_unknown_fields() {
    // Given
    let yaml = r#"
schema_version: 1
bogus_top_level_key: 123
targets: []
"#;

    // When / Then
    let result = load_build_manifest(yaml);
    assert!(
        result.is_err(),
        "unknown manifest fields must be rejected, got: {result:?}"
    );
}

#[test]
fn cache_key_is_deterministic() {
    // Given
    let action = BuildAction {
        id: "compile".to_string(),
        description: String::new(),
        r#type: ActionType::Command as i32,
        command: vec!["echo".to_string(), "hi".to_string()],
        env: HashMap::new(),
        inputs: vec![],
        outputs: vec![],
        tool_dep_ids: vec![],
        working_dir: String::new(),
    };
    let fingerprints = vec![FileFingerprint {
        path: "src/a.txt".to_string(),
        size: 10,
        mtime_ms: 123,
    }];

    // When
    let k1 = compute_cache_key(&action, &fingerprints);
    let k2 = compute_cache_key(&action, &fingerprints);

    // Then
    assert_eq!(k1, k2, "same action + inputs must produce the same key");
    assert!(
        k1.starts_with("sha256:"),
        "key must be sha256-prefixed: {k1}"
    );
}

/// A target whose single action appends a line to `marker.txt`, with a declared
/// input + output so the cache can fingerprint and verify it.
const CACHE_DEMO_YAML: &str = r#"
schema_version: 1
targets:
  - id: "cache:demo"
    name: "Cache Demo"
    actions:
      - id: "write"
        description: "append a marker line"
        type: command
        command: ["sh", "-c", "echo run >> marker.txt"]
        inputs:
          - include: ["input.txt"]
            root: "."
        outputs:
          - path: "marker.txt"
            kind: file
"#;

#[tokio::test]
async fn cache_hit_skips_execution() {
    // Given
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");
    let manifest = load_build_manifest(CACHE_DEMO_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();

    // When
    let first = execute_target(
        root,
        &graph,
        "cache:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("first run");

    // Then
    assert!(!first.actions[0].cached, "first run must execute");

    // When
    let second = execute_target(
        root,
        &graph,
        "cache:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("second run");

    // Then
    assert!(second.actions[0].cached, "second run must be a cache hit");
    let marker = std::fs::read_to_string(root.join("marker.txt")).expect("marker.txt");
    assert_eq!(marker.lines().count(), 1, "script must run exactly once");
}

#[tokio::test]
async fn cache_miss_on_input_mtime_change() {
    // Given
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");
    let manifest = load_build_manifest(CACHE_DEMO_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();

    // When
    let first = execute_target(
        root,
        &graph,
        "cache:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("first run");

    // Then
    assert!(!first.actions[0].cached);

    // Given — change the input so its fingerprint differs.
    std::fs::write(root.join("input.txt"), "seed-changed-larger").expect("rewrite input");

    // When
    let second = execute_target(
        root,
        &graph,
        "cache:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("second run");

    // Then
    assert!(
        !second.actions[0].cached,
        "changed input must invalidate the cache"
    );
    let marker = std::fs::read_to_string(root.join("marker.txt")).expect("marker.txt");
    assert_eq!(
        marker.lines().count(),
        2,
        "script must run again after input change"
    );
}

#[tokio::test]
async fn build_executes_script_target() {
    // Given
    let yaml = r#"
schema_version: 1
targets:
  - id: "hello:script"
    name: "Hello"
    config:
      type: script
      command: ["echo", "hello-from-script"]
"#;
    let repo = tempfile::tempdir().expect("tempdir");
    let manifest = load_build_manifest(yaml).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");

    // When
    let record = execute_target(
        repo.path(),
        &graph,
        "hello:script",
        &ExecuteOptions::default(),
        tddy_build::BuildMode::Compile,
        &PluginRegistry::new(),
    )
    .await
    .expect("run script target");

    // Then
    assert_eq!(record.actions[0].exit_code, 0);
    assert!(
        record.actions[0].stdout.contains("hello-from-script"),
        "stdout must capture the script output, got: {:?}",
        record.actions[0].stdout
    );
}

#[test]
fn cycle_detection_returns_error() {
    // Given
    let yaml = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["b:t"]
    config:
      type: script
      command: ["true"]
  - id: "b:t"
    name: B
    deps: ["a:t"]
    config:
      type: script
      command: ["true"]
"#;
    let manifest = load_build_manifest(yaml).expect("parse manifest");

    // When / Then
    let result = BuildGraph::from_manifests(vec![manifest]);
    assert!(result.is_err(), "a↔b dependency cycle must be rejected");
}

#[test]
fn build_action_dag_parallel_wave_ordering() {
    // Given — A and B are independent (parallel); C consumes both their outputs.
    let yaml = r#"
schema_version: 1
targets:
  - id: "fan:in"
    name: "Fan In"
    actions:
      - id: "a"
        type: command
        command: ["sh", "-c", "echo a > a.out"]
        outputs:
          - path: "a.out"
            kind: file
      - id: "b"
        type: command
        command: ["sh", "-c", "echo b > b.out"]
        outputs:
          - path: "b.out"
            kind: file
      - id: "c"
        type: command
        command: ["sh", "-c", "cat a.out b.out > c.out"]
        inputs:
          - include: ["a.out", "b.out"]
            root: "."
        outputs:
          - path: "c.out"
            kind: file
"#;
    let manifest = load_build_manifest(yaml).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");

    // When
    let waves = graph.waves(&PluginRegistry::new()).expect("compute waves");

    // Then
    assert_eq!(waves.len(), 2, "expected two waves: [a,b] then [c]");
    assert_eq!(
        waves[0].len(),
        2,
        "a and b run in parallel in the first wave"
    );
    assert_eq!(waves[1].len(), 1, "c runs alone after a and b");
}
