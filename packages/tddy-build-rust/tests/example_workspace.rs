//! Exercises the rust recipe plugin on a real, interdependent cargo workspace:
//! deps-first ordering, real `cargo build`, and the action cache (hit on rerun,
//! miss after a source edit). Skips with a notice when cargo is unavailable.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_rust::RustPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/workspace")
}

fn cargo_available() -> bool {
    std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(RustPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        if from.file_name().map(|n| n == "target").unwrap_or(false) {
            continue; // never copy build artifacts
        }
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn rust_targets_depend_on_each_other_deps_first() {
    // Given
    let graph = load(&example_root());

    // When
    let order = graph.build_order("mathapp:bin").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");

    // Then
    assert!(pos("mathcore:lib") < pos("mathutil:lib"));
    assert!(pos("mathutil:lib") < pos("mathapp:bin"));
}

#[test]
fn rust_plugin_lowers_expected_cargo_argv() {
    // Given
    let graph = load(&example_root());

    // When
    let actions = graph
        .actions_for("mathapp:bin", &registry())
        .expect("lower");

    // Then
    assert_eq!(
        actions[0].command,
        vec!["cargo", "build", "-p", "mathapp", "--bin", "mathapp"]
    );
}

#[tokio::test]
async fn rust_workspace_builds_with_real_cargo() {
    if !cargo_available() {
        eprintln!("SKIP: cargo not available");
        return;
    }

    // Given
    let dir = staged();
    let graph = load(dir.path());

    // When
    let record = execute_target(
        dir.path(),
        &graph,
        "mathapp:bin",
        &ExecuteOptions::default(),
        tddy_build::BuildMode::Compile,
        &registry(),
    )
    .await
    .expect("cargo build");

    // Then
    assert_eq!(
        record.actions[0].exit_code, 0,
        "stderr: {}",
        record.actions[0].stderr
    );
    assert!(
        dir.path().join("target/debug/mathapp").exists(),
        "binary produced"
    );
}

#[tokio::test]
async fn rust_cache_hits_then_misses_after_source_edit() {
    if !cargo_available() {
        eprintln!("SKIP: cargo not available");
        return;
    }

    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    // When
    let first = execute_target(
        dir.path(),
        &graph,
        "mathcore:lib",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("first");

    // Then
    assert!(!first.actions[0].cached);

    // When (rerun without changes)
    let second = execute_target(
        dir.path(),
        &graph,
        "mathcore:lib",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("second");

    // Then
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // When (source file edited)
    std::fs::write(
        dir.path().join("mathcore/src/lib.rs"),
        "pub fn add(a: i64, b: i64) -> i64 { a + b + 0 }\n",
    )
    .expect("edit source");
    let third = execute_target(
        dir.path(),
        &graph,
        "mathcore:lib",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("third");

    // Then
    assert!(
        !third.actions[0].cached,
        "source edit invalidates the cache"
    );
}

#[test]
fn rust_typed_cycle_is_detected() {
    // Given
    let yaml = r#"
schema_version: 1
targets:
  - id: "x:lib"
    name: X
    deps: ["y:lib"]
    config: { type: rust_library, package: x }
  - id: "y:lib"
    name: Y
    deps: ["x:lib"]
    config: { type: rust_library, package: y }
"#;
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");

    // When / Then
    assert!(
        BuildGraph::from_manifests(vec![manifest]).is_err(),
        "a cycle between plugin-typed targets must be rejected"
    );
}
