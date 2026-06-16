//! Acceptance tests for the tddy-build crate.
//!
//! These pin the externally-observable contract: YAML→proto deserialization, the
//! content-addressed action cache, DAG construction, and target execution.

use std::collections::HashMap;

use tddy_build::cache::compute_cache_key;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::load_build_manifest;
use tddy_build::proto::{build_target, ActionType, BuildAction, FileFingerprint};

/// A manifest exercising one of each of the seven target types.
const ALL_TYPES_YAML: &str = r#"
schema_version: 1
targets:
  - id: "app:bin"
    name: "App Binary"
    config:
      type: rust_binary
      package: app
      bin_name: app
      features: [foo]
      profile: release
  - id: "lib:core"
    name: "Core Lib"
    config:
      type: rust_library
      package: core
      features: []
      profile: debug
  - id: "web:dist"
    name: "Web"
    config:
      type: typescript
      package_dir: packages/web
      build_script: build
      output_dirs: [dist]
  - id: "img:app"
    name: "Image"
    config:
      type: docker_image
      dockerfile: Dockerfile
      context: "."
      tag: "app:latest"
  - id: "foo:script"
    name: "Script"
    config:
      type: script
      command: [echo, hello]
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
      member_ids: ["app:bin", "lib:core"]
"#;

#[test]
fn build_manifest_yaml_round_trips_all_target_types() {
    let manifest = load_build_manifest(ALL_TYPES_YAML).expect("manifest must parse");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.targets.len(), 7);

    let by_id = |id: &str| {
        manifest
            .targets
            .iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("missing target {id}"))
            .config
            .as_ref()
            .unwrap_or_else(|| panic!("target {id} has no config"))
    };

    match by_id("app:bin") {
        build_target::Config::RustBinary(rb) => {
            assert_eq!(rb.package, "app");
            assert_eq!(rb.bin_name, "app");
            assert_eq!(rb.features, vec!["foo".to_string()]);
            assert_eq!(rb.profile, "release");
        }
        _ => panic!("app:bin must be rust_binary"),
    }
    match by_id("lib:core") {
        build_target::Config::RustLibrary(rl) => assert_eq!(rl.package, "core"),
        _ => panic!("lib:core must be rust_library"),
    }
    match by_id("web:dist") {
        build_target::Config::Typescript(ts) => {
            assert_eq!(ts.package_dir, "packages/web");
            assert_eq!(ts.build_script, "build");
            assert_eq!(ts.output_dirs, vec!["dist".to_string()]);
        }
        _ => panic!("web:dist must be typescript"),
    }
    match by_id("img:app") {
        build_target::Config::DockerImage(d) => {
            assert_eq!(d.dockerfile, "Dockerfile");
            assert_eq!(d.tag, "app:latest");
        }
        _ => panic!("img:app must be docker_image"),
    }
    match by_id("foo:script") {
        build_target::Config::Script(s) => {
            assert_eq!(s.command, vec!["echo".to_string(), "hello".to_string()]);
        }
        _ => panic!("foo:script must be script"),
    }
    match by_id("tools:bin") {
        build_target::Config::Tool(t) => {
            assert_eq!(t.bin_dir, "tools/bin");
            assert_eq!(t.commands.get("greet").map(String::as_str), Some("greet"));
        }
        _ => panic!("tools:bin must be tool"),
    }
    match by_id("all:group") {
        build_target::Config::Group(g) => {
            assert_eq!(
                g.member_ids,
                vec!["app:bin".to_string(), "lib:core".to_string()]
            );
        }
        _ => panic!("all:group must be group"),
    }
}

#[test]
fn build_manifest_rejects_unknown_fields() {
    let yaml = r#"
schema_version: 1
bogus_top_level_key: 123
targets: []
"#;
    let result = load_build_manifest(yaml);
    assert!(
        result.is_err(),
        "unknown manifest fields must be rejected, got: {result:?}"
    );
}

#[test]
fn cache_key_is_deterministic() {
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
        path: "src/a.rs".to_string(),
        size: 10,
        mtime_ms: 123,
    }];

    let k1 = compute_cache_key(&action, &fingerprints);
    let k2 = compute_cache_key(&action, &fingerprints);
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
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");
    let manifest = load_build_manifest(CACHE_DEMO_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let opts = ExecuteOptions::default();

    let first = execute_target(root, &graph, "cache:demo", &opts)
        .await
        .expect("first run");
    assert!(!first.actions[0].cached, "first run must execute");

    let second = execute_target(root, &graph, "cache:demo", &opts)
        .await
        .expect("second run");
    assert!(second.actions[0].cached, "second run must be a cache hit");

    let marker = std::fs::read_to_string(root.join("marker.txt")).expect("marker.txt");
    assert_eq!(marker.lines().count(), 1, "script must run exactly once");
}

#[tokio::test]
async fn cache_miss_on_input_mtime_change() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");
    let manifest = load_build_manifest(CACHE_DEMO_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let opts = ExecuteOptions::default();

    let first = execute_target(root, &graph, "cache:demo", &opts)
        .await
        .expect("first run");
    assert!(!first.actions[0].cached);

    // Change the input so its fingerprint differs.
    std::fs::write(root.join("input.txt"), "seed-changed-larger").expect("rewrite input");

    let second = execute_target(root, &graph, "cache:demo", &opts)
        .await
        .expect("second run");
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

    let record = execute_target(
        repo.path(),
        &graph,
        "hello:script",
        &ExecuteOptions::default(),
    )
    .await
    .expect("run script target");
    assert_eq!(record.actions[0].exit_code, 0);
    assert!(
        record.actions[0].stdout.contains("hello-from-script"),
        "stdout must capture the script output, got: {:?}",
        record.actions[0].stdout
    );
}

#[tokio::test]
async fn build_respects_tool_target_bin_dir() {
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

    let record = execute_target(root, &graph, "uses:tool", &ExecuteOptions::default())
        .await
        .expect("run tool-dependent target");
    assert!(
        record.actions[0].stdout.contains("GREETED-BY-TOOL"),
        "tool bin_dir must be on PATH, got: {:?}",
        record.actions[0].stdout
    );
}

#[test]
fn cycle_detection_returns_error() {
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
    let result = BuildGraph::from_manifests(vec![manifest]);
    assert!(result.is_err(), "a↔b dependency cycle must be rejected");
}

#[test]
fn build_action_dag_parallel_wave_ordering() {
    // A and B are independent (parallel); C consumes both their outputs.
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
    let waves = graph.waves().expect("compute waves");

    assert_eq!(waves.len(), 2, "expected two waves: [a,b] then [c]");
    assert_eq!(
        waves[0].len(),
        2,
        "a and b run in parallel in the first wave"
    );
    assert_eq!(waves[1].len(), 1, "c runs alone after a and b");
}

#[tokio::test]
async fn dry_run_emits_argv_for_all_seven_target_types() {
    let repo = tempfile::tempdir().expect("tempdir");
    let manifest = load_build_manifest(ALL_TYPES_YAML).expect("parse manifest");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("build graph");
    let opts = ExecuteOptions {
        dry_run: true,
        ..ExecuteOptions::default()
    };

    // Command-producing types: assert the lowered program (argv[0]).
    let expectations: &[(&str, &str)] = &[
        ("app:bin", "cargo"),
        ("lib:core", "cargo"),
        ("web:dist", "bun"),
        ("img:app", "docker"),
        ("foo:script", "echo"),
    ];
    for (target_id, program) in expectations {
        let record = execute_target(repo.path(), &graph, target_id, &opts)
            .await
            .unwrap_or_else(|e| panic!("dry-run {target_id} failed: {e}"));
        assert!(
            !record.actions.is_empty(),
            "{target_id} must lower to at least one action"
        );
        assert_eq!(
            record.actions[0].argv.first().map(String::as_str),
            Some(*program),
            "{target_id} argv[0] should be {program}"
        );
    }

    // tool / group do not themselves emit a build command — they must still
    // dry-run without error.
    for target_id in ["tools:bin", "all:group"] {
        execute_target(repo.path(), &graph, target_id, &opts)
            .await
            .unwrap_or_else(|e| panic!("dry-run {target_id} failed: {e}"));
    }

    // dry-run must not execute anything.
    assert!(
        !repo.path().join("marker.txt").exists(),
        "dry-run must not produce build artifacts"
    );
}
